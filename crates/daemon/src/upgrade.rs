//! Replacing and repairing the installed service without losing or
//! duplicating charged work (DG1-C11).
//!
//! An upgrade replaces the current release with a staged one. It checks that
//! the new release can serve the same consumers and read the same journal,
//! closes admission at the running service (cancelling Prepared attempts)
//! and waits until nothing is charged. It then stops the service, takes a
//! quiescent backup of the journal, the selection and the current manifest,
//! starts the new release with admission still closed, verifies the running
//! binary, the handshake and the state, and only then reopens admission. A
//! drain that does not finish in time is abandoned: admission reopens on the
//! current release and every charge is kept. If the new release cannot be
//! verified, the previous one is started again on the same journal; a backup
//! is never restored over state a release may have admitted from.
//!
//! The release an upgrade replaces stays selected as the last known good
//! one. Repair returns the service to it while no authority serves: it never
//! starts a second authority, keeps a journal that cannot be opened closed,
//! refuses a release that cannot serve the journal, and uses the recovery
//! copy when the installed release is damaged.

use crate::install::{
    self, compiled, digest_file, freeze_copy, handshake, observe_running, read_release,
    read_selection, render_plist, unix_ms, validate_package, wait_running, write_selection,
    BuildCompatibility, InstallOptions, Package, RunningEvidence, Selection, SelectionEvent,
    ServiceManager, ServiceSpec,
};
use crate::paths::{read_private, replace_private, secure_directory, AuthorityPaths};
use devguard_client::protocol::{AdmissionClosure, CallerCredential, Quiescence};
use devguard_client::Client;
use devguard_contract::{Capability, Compatibility, Error, ErrorCode, Result, Secret};
use devguard_core::AuthorityStorage;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The default time a drain may take before the upgrade is abandoned.
pub const DEFAULT_DRAIN_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a stopped service may take to release the endpoint and the lock.
const STOP_LIMIT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(200);
/// What every consumer of the service requires; a release without it cannot
/// replace the current one.
pub const CONSUMER_REQUIREMENTS: [Capability; 2] =
    [Capability::DurableAdmission, Capability::FencedLaunch];

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidRequest, message)
}

fn unavailable(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

fn busy(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceUnavailable, message)
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourcePolicyUnsupported, message)
}

fn macos_only() -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        Err(unsupported("the service is managed as a macOS LaunchAgent"))
    }
}

/// The running program must be `artifact` of `release`, so a release's
/// binaries are never mixed with another build's.
fn running_from(release: &Package, artifact: &str) -> Result<()> {
    let running =
        std::env::current_exe().map_err(|_| unavailable("cannot locate the running program"))?;
    if digest_file(&running)? != release.manifest.artifacts[artifact].sha256 {
        return Err(invalid(format!(
            "run this from the release it selects: its {artifact} must be this program"
        )));
    }
    Ok(())
}

/// The release under `releases/`, by a plain id.
fn release_dir(paths: &AuthorityPaths, id: &str) -> Result<PathBuf> {
    if id.is_empty()
        || id.starts_with('.')
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(invalid("a release id is a plain name"));
    }
    Ok(paths.releases().join(id))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StageReport {
    pub release_id: String,
    pub release: PathBuf,
    pub manifest_sha256: String,
    pub reused_release: bool,
}

/// Copy `package` into the releases as an immutable release, ready for an
/// upgrade. The service, the selection and the journal are untouched. It must
/// run from the package, as installation does.
pub fn stage(paths: &AuthorityPaths, package: &Path) -> Result<StageReport> {
    macos_only()?;
    let package = validate_package(package)?;
    running_from(&package, "devguardd")?;
    secure_directory(&paths.releases(), paths.uid(), true)?;
    install::sweep_incoming(&paths.releases());
    let id = package.manifest.release_id.clone();
    let release = paths.releases().join(&id);
    let reused = freeze_copy(&package, &release)?;
    let staged = validate_package(&release)?;
    Ok(StageReport {
        release_id: id,
        release,
        manifest_sha256: staged.manifest_sha256,
        reused_release: reused,
    })
}

/// What changes between the current release and its replacement.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub from: BuildCompatibility,
    pub to: BuildCompatibility,
    /// Capabilities the current release states that the replacement lacks.
    pub dropped: BTreeSet<Capability>,
    /// The replacement states less than the current release. It is allowed
    /// only when it reads the same journal and serves the same consumers.
    pub downgrade: bool,
}

/// Whether `to` can replace `from`: the same wire version, protocol, journal
/// schema and configuration schema, and everything consumers require. A
/// replacement that states less is a downgrade; an incompatible one is refused.
pub fn check_compatibility(
    from: &BuildCompatibility,
    to: &BuildCompatibility,
) -> Result<CompatibilityReport> {
    if from.wire_version != to.wire_version || from.protocol != to.protocol {
        return Err(unsupported(
            "the releases speak different wire versions or protocols; the replacement is refused",
        ));
    }
    if from.journal_schema != to.journal_schema {
        return Err(unsupported(format!(
            "the replacement reads journal schema {}, not {}; an incompatible downgrade or an unsupported migration is refused",
            to.journal_schema, from.journal_schema
        )));
    }
    if from.config_schema != to.config_schema {
        return Err(unsupported(
            "the replacement reads another configuration schema; the replacement is refused",
        ));
    }
    let missing: Vec<Capability> = CONSUMER_REQUIREMENTS
        .into_iter()
        .filter(|capability| !to.capabilities.contains(capability))
        .collect();
    if !missing.is_empty() {
        return Err(unsupported(format!(
            "the replacement lacks {missing:?}, which consumers require; the replacement is refused"
        )));
    }
    let dropped: BTreeSet<Capability> = from
        .capabilities
        .difference(&to.capabilities)
        .copied()
        .collect();
    Ok(CompatibilityReport {
        from: from.clone(),
        to: to.clone(),
        downgrade: !dropped.is_empty(),
        dropped,
    })
}

/// Options of one upgrade.
#[derive(Debug, Clone)]
pub struct UpgradeOptions {
    pub drain_timeout: Duration,
    /// Replace a running release that cannot close admission by stopping it
    /// first; the replacement proceeds only if nothing is then charged.
    pub stopped: bool,
}

impl Default for UpgradeOptions {
    fn default() -> Self {
        Self {
            drain_timeout: DEFAULT_DRAIN_TIMEOUT,
            stopped: false,
        }
    }
}

/// How the running service was brought to rest.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DrainMode {
    /// It closed admission and drained while serving.
    ClosedAdmission,
    /// It could not close admission, so it was stopped and its journal checked.
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrainReport {
    pub mode: DrainMode,
    pub waited_ms: u64,
    /// What the service reported when admission closed.
    pub at_close: Option<Quiescence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupReport {
    pub directory: PathBuf,
    /// File name to SHA-256.
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpgradeReport {
    pub from: String,
    pub to: String,
    pub compatibility: CompatibilityReport,
    pub drain: DrainReport,
    pub backup: BackupReport,
    pub running: RunningEvidence,
    /// What the new release reported before admission reopened.
    pub before_reopening: Quiescence,
    pub recovery: PathBuf,
    pub selection: Selection,
}

fn admin_secret(paths: &AuthorityPaths) -> Result<Secret> {
    let bytes = read_private(&paths.admin_credential(), paths.uid(), 1024)?;
    Secret::new(
        String::from_utf8(bytes)
            .map_err(|_| Error::new(ErrorCode::Unauthorized, "the admin credential is invalid"))?,
    )
}

/// A new administrator session that requires the drain capability.
fn administrator(paths: &AuthorityPaths, secret: &Secret) -> Result<Client> {
    let mut client = Client::connect(
        &paths.socket(),
        paths.uid(),
        Compatibility {
            minimum_protocol: 1,
            maximum_protocol: 1,
            required: BTreeSet::from([Capability::UpgradeDrain]),
        },
    )?;
    client.authenticate(CallerCredential::Administrator {
        secret: secret.clone(),
    })?;
    Ok(client)
}

/// A call that is retried once when its reply was lost; every drain call is
/// idempotent.
fn admin_call<T>(
    paths: &AuthorityPaths,
    secret: &Secret,
    call: impl Fn(&mut Client) -> Result<T>,
) -> Result<T> {
    let attempt = || administrator(paths, secret).and_then(|mut client| call(&mut client));
    match attempt() {
        Err(error) if error.code == ErrorCode::ResourceControlUnavailable => attempt(),
        other => other,
    }
}

/// The handshake every consumer makes must succeed.
fn consumer_handshake(paths: &AuthorityPaths) -> Result<()> {
    Client::connect(
        &paths.socket(),
        paths.uid(),
        Compatibility {
            minimum_protocol: 1,
            maximum_protocol: 1,
            required: CONSUMER_REQUIREMENTS.into_iter().collect(),
        },
    )
    .map(drop)
}

fn spec_for(release: &Package, options: &InstallOptions) -> ServiceSpec {
    ServiceSpec {
        label: options.label.clone(),
        plist: options.plist.clone(),
        program: release.dir.join("bin/devguardd"),
        args: options.program_args.clone(),
        environment: options.environment.clone(),
    }
}

/// Replace the plist, which is not private, atomically.
fn write_plist(spec: &ServiceSpec, options: &InstallOptions) -> Result<()> {
    let text = render_plist(spec, &options.log, options.throttle_seconds)?;
    let parent = options
        .plist
        .parent()
        .ok_or_else(|| invalid("the plist needs a directory"))?;
    install::ensure_plain_directory(parent, 0o755)?;
    let temporary = parent.join(format!(
        ".{}.{}",
        options.label,
        uuid::Uuid::new_v4().simple()
    ));
    let written = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, &options.plist)?;
        File::open(parent)?.sync_all()
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(unavailable("cannot write the service plist"));
    }
    Ok(())
}

/// Stop the service and wait until it has released the endpoint and the
/// authority lock; the returned storage holds the lock.
fn stop(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
) -> Result<AuthorityStorage> {
    manager.bootout(&options.label)?;
    let deadline = Instant::now() + STOP_LIMIT;
    loop {
        let last = if handshake(paths).is_ok() {
            busy("an authority still serves after the service was stopped")
        } else {
            match AuthorityStorage::open(&paths.journal()) {
                Ok(storage) => return Ok(storage),
                Err(error) => error,
            }
        };
        if Instant::now() >= deadline {
            return Err(last);
        }
        std::thread::sleep(POLL);
    }
}

/// Start `release` as the service and verify that launchd runs it.
fn start(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
    release: &Package,
) -> Result<RunningEvidence> {
    let spec = spec_for(release, options);
    write_plist(&spec, options)?;
    let started = manager
        .bootstrap(&spec)
        .and_then(|()| wait_running(paths, manager, options, release));
    if started.is_err() {
        let _ = manager.bootout(&options.label);
    }
    started
}

fn write_marker(paths: &AuthorityPaths, reason: &str) -> Result<()> {
    let closure = AdmissionClosure {
        reason: reason.into(),
        since_unix_ms: unix_ms() as u64,
    };
    let bytes =
        serde_json::to_vec(&closure).map_err(|_| unavailable("cannot encode the closure"))?;
    replace_private(&paths.admission_marker(), &bytes)
}

fn remove_marker(paths: &AuthorityPaths) -> Result<()> {
    match fs::remove_file(paths.admission_marker()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(unavailable("cannot remove the admission marker")),
    }
}

/// Reopen admission on whatever release now serves: through the service when
/// it can, or by removing the marker when it cannot read one.
fn reopen(paths: &AuthorityPaths, secret: &Secret) -> Result<()> {
    match admin_call(paths, secret, |client| client.open_admission()) {
        Ok(_) => Ok(()),
        Err(error) if error.code == ErrorCode::ResourcePolicyUnsupported => remove_marker(paths),
        Err(error) => Err(error),
    }
}

/// Take a quiescent backup while `storage` holds the lock: the journal, the
/// selection and the current release's manifest, each hashed.
fn backup(
    paths: &AuthorityPaths,
    storage: &mut AuthorityStorage,
    current: &Package,
    to: &str,
) -> Result<BackupReport> {
    secure_directory(&paths.backups(), paths.uid(), true)?;
    let directory = paths.backups().join(format!(
        "{}-{}-to-{}",
        unix_ms(),
        current.manifest.release_id,
        to
    ));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|_| unavailable("cannot create the backup directory"))?;
    let journal = directory.join("authority.sqlite");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&journal)
        .map_err(|_| unavailable("cannot create the journal backup"))?;
    storage.backup(&journal)?;
    let mut copies = vec![(
        "MANIFEST.json".to_string(),
        current.dir.join("MANIFEST.json"),
    )];
    if fs::symlink_metadata(paths.selection()).is_ok() {
        copies.push(("selection.json".into(), paths.selection()));
    }
    for (name, from) in copies {
        let bytes = fs::read(&from).map_err(|_| unavailable("cannot read a backed-up file"))?;
        crate::paths::write_new_private(&directory.join(&name), &bytes)?;
    }
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(&directory)
        .map_err(|_| unavailable("cannot list the backup"))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        files.insert(name, digest_file(&entry.path())?);
    }
    File::open(&directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| unavailable("cannot sync the backup"))?;
    Ok(BackupReport { directory, files })
}

/// Close admission at the running service and wait until nothing is charged.
/// On a timeout admission reopens and the charges are kept.
fn drain(
    paths: &AuthorityPaths,
    secret: &Secret,
    reason: &str,
    timeout: Duration,
) -> Result<(Quiescence, Duration)> {
    let started = Instant::now();
    // Whatever interrupts the drain, admission does not stay closed.
    let reopened = |error: Error| match reopen(paths, secret) {
        Ok(()) => error,
        Err(reopening) => Error::new(
            error.code,
            format!(
                "{}; reopening admission also failed ({})",
                error.message, reopening.message
            ),
        ),
    };
    let at_close = admin_call(paths, secret, |client| {
        client.close_admission(reason.into())
    })
    .map_err(reopened)?;
    let deadline = started + timeout;
    loop {
        let now = admin_call(paths, secret, |client| client.quiescence()).map_err(reopened)?;
        if now.quiet() {
            return Ok((at_close, started.elapsed()));
        }
        if Instant::now() >= deadline {
            reopen(paths, secret)?;
            return Err(busy(format!(
                "the drain did not finish in {} s: {} attempts and {} leases are still charged; admission reopened on the current release, which keeps them",
                timeout.as_secs(),
                now.attempts.len(),
                now.leases.len()
            )));
        }
        std::thread::sleep(POLL);
    }
}

/// Start the current release again after a failed replacement, on the same
/// journal, and reopen admission.
fn restore(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
    current: &Package,
    secret: &Secret,
    cause: Error,
) -> Error {
    let restored = start(paths, manager, options, current).and_then(|_| reopen(paths, secret));
    match restored {
        Ok(()) => Error::new(
            cause.code,
            format!(
                "{}; release {} serves again with admission open",
                cause.message, current.manifest.release_id
            ),
        ),
        Err(error) => Error::new(
            ErrorCode::ReconciliationRequired,
            format!(
                "{}; restarting release {} also failed ({}): run `devguard repair --use last-known-good`",
                cause.message, current.manifest.release_id, error.message
            ),
        ),
    }
}

/// Replace the current release with the staged release `to`, run from that
/// release's own `devguard`.
pub fn upgrade(
    paths: &AuthorityPaths,
    to: &str,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
    upgrade: &UpgradeOptions,
) -> Result<UpgradeReport> {
    macos_only()?;
    paths.validate_existing()?;
    let mut selection =
        read_selection(paths)?.ok_or_else(|| invalid("no release is installed; install one"))?;
    let from = selection.current.clone();
    if from == to {
        return Err(invalid(format!("release {to} is already current")));
    }
    let target = validate_package(&release_dir(paths, to)?)
        .map_err(|error| Error::new(error.code, format!("release {to}: {}", error.message)))?;
    running_from(&target, "devguard")?;
    // The current release runs from its installed directory or, after a
    // repair, from its recovery copy; both hold the same manifest.
    let copies: Vec<Package> = [release_dir(paths, &from)?, paths.recovery().join(&from)]
        .iter()
        .filter_map(|dir| read_release(dir).ok())
        .collect();
    let manifest = copies.first().ok_or_else(|| {
        Error::new(
            ErrorCode::ReconciliationRequired,
            format!("the current release {from} is not intact; use repair"),
        )
    })?;
    let compatibility = check_compatibility(
        &manifest.manifest.compatibility,
        &target.manifest.compatibility,
    )?;
    // Whichever copy runs is the one started again if the upgrade fails.
    let current = copies
        .into_iter()
        .find(|copy| observe_running(paths, manager, &options.label, copy).is_ok())
        .ok_or_else(|| {
            busy(format!(
                "the current release {from} is not running verified; use repair"
            ))
        })?;
    // The recovery copy is made before anything changes; repair uses it only
    // once the release has been verified running.
    secure_directory(&paths.recovery(), paths.uid(), true)?;
    let recovery = paths.recovery().join(to);
    freeze_copy(&target, &recovery)?;
    let secret = admin_secret(paths)?;
    let reason = format!("upgrade from {from} to {to}");
    // A release that cannot close admission refuses the capability itself.
    let drainable = match administrator(paths, &secret) {
        Ok(_) => true,
        Err(error) if error.code == ErrorCode::ResourcePolicyUnsupported => false,
        Err(error) => return Err(error),
    };
    let (mode, at_close, waited) = if drainable {
        let (at_close, waited) = drain(paths, &secret, &reason, upgrade.drain_timeout)?;
        (DrainMode::ClosedAdmission, Some(at_close), waited)
    } else if upgrade.stopped {
        (DrainMode::Stopped, None, Duration::ZERO)
    } else {
        return Err(unsupported(format!(
            "release {from} cannot close admission; rerun with --stopped to stop it first, which proceeds only if nothing is then charged"
        )));
    };
    let mut storage = match stop(paths, manager, options) {
        Ok(storage) => storage,
        Err(error) => {
            return Err(restore(paths, manager, options, &current, &secret, error));
        }
    };
    let (attempts, leases) = match storage.charged() {
        Ok(charged) => charged,
        Err(error) => {
            drop(storage);
            return Err(restore(paths, manager, options, &current, &secret, error));
        }
    };
    if !attempts.is_empty() || !leases.is_empty() {
        drop(storage);
        let cause = busy(format!(
            "{} attempts and {} leases are still charged; nothing was replaced",
            attempts.len(),
            leases.len()
        ));
        return Err(restore(paths, manager, options, &current, &secret, cause));
    }
    let backup = match backup(paths, &mut storage, &current, to) {
        Ok(backup) => backup,
        Err(error) => {
            drop(storage);
            return Err(restore(paths, manager, options, &current, &secret, error));
        }
    };
    // The new release starts with admission closed and opens it only once
    // it has been verified.
    let marked = write_marker(paths, &reason);
    drop(storage);
    if let Err(error) = marked {
        return Err(restore(paths, manager, options, &current, &secret, error));
    }
    let verified = start(paths, manager, options, &target).and_then(|running| {
        consumer_handshake(paths)?;
        let before = admin_call(paths, &secret, |client| client.quiescence())?;
        if before.closure.is_none() || !before.quiet() {
            return Err(unavailable(
                "the new release did not start closed and idle on the journal",
            ));
        }
        Ok((running, before))
    });
    let (running, before_reopening) = match verified {
        Ok(verified) => verified,
        Err(error) => {
            let _ = manager.bootout(&options.label);
            return Err(restore(paths, manager, options, &current, &secret, error));
        }
    };
    // Verified: select it, keep the release it replaced as the one repair
    // returns to, then reopen admission.
    selection.current = to.into();
    selection.last_known_good = Some(from.clone());
    selection.history.push(SelectionEvent {
        release_id: to.into(),
        event: format!(
            "upgraded from {from} and verified running; {from} stays the last known good release"
        ),
        unix_ms: unix_ms(),
    });
    write_selection(paths, &selection).map_err(|error| {
        Error::new(
            error.code,
            format!(
                "release {to} serves with admission closed, but the selection could not be recorded ({}); run the upgrade again",
                error.message
            ),
        )
    })?;
    admin_call(paths, &secret, |client| client.open_admission()).map_err(|error| {
        Error::new(
            error.code,
            format!(
                "release {to} serves and is selected, but admission could not be reopened: {}",
                error.message
            ),
        )
    })?;
    Ok(UpgradeReport {
        from,
        to: to.into(),
        compatibility,
        drain: DrainReport {
            mode,
            waited_ms: waited.as_millis() as u64,
            at_close,
        },
        backup,
        running,
        before_reopening,
        recovery,
        selection,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairReport {
    pub release_id: String,
    /// The installed release, or its recovery copy when that was damaged.
    pub artifact: PathBuf,
    pub from_recovery: bool,
    /// Why the installed release was not used, when it was not.
    pub damaged: Option<String>,
    /// The repaired release against this build, which read the journal.
    pub compatibility: CompatibilityReport,
    pub running: RunningEvidence,
    /// An admission closure left by an interrupted upgrade, now reopened.
    pub reopened_closure: bool,
    pub selection: Selection,
}

/// Whether the last known good release can serve a journal that this build
/// opened: the same schemas and consumers, and parent leases if the journal
/// holds unreleased ones.
pub fn repair_compatibility(
    this: &BuildCompatibility,
    release: &BuildCompatibility,
    unreleased_leases: usize,
) -> Result<CompatibilityReport> {
    let report = check_compatibility(this, release)?;
    if unreleased_leases > 0 && !release.capabilities.contains(&Capability::ParentLease) {
        return Err(unsupported(format!(
            "the release does not account parent leases, and the journal holds {unreleased_leases} unreleased"
        )));
    }
    Ok(report)
}

/// Return the service to the last known good release, or its recovery copy,
/// while no authority serves. The service runs only that release's own
/// binaries, whichever `devguard` runs the repair.
pub fn repair(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
) -> Result<RepairReport> {
    macos_only()?;
    paths.validate_existing()?;
    let mut selection =
        read_selection(paths)?.ok_or_else(|| invalid("no release is installed; install one"))?;
    let id = selection
        .last_known_good
        .clone()
        .ok_or_else(|| invalid("no release has been verified running yet"))?;
    if handshake(paths).is_ok() {
        return Err(busy(
            "an authority is serving; repair never starts a second one, and a serving release is replaced by an upgrade",
        ));
    }
    // The journal must open: repair never creates, resets or restores it.
    let closed = |error: Error| {
        Error::new(
            error.code,
            format!(
                "the journal cannot be opened ({}); admission stays closed and repair never reinitializes it",
                error.message
            ),
        )
    };
    let mut storage = AuthorityStorage::open(&paths.journal()).map_err(closed)?;
    let (_, leases) = storage.charged().map_err(closed)?;
    drop(storage);
    let (release, damaged) = match read_release(&release_dir(paths, &id)?) {
        Ok(release) => (release, None),
        Err(error) => {
            let copy = read_release(&paths.recovery().join(&id)).map_err(|recovery| {
                Error::new(
                    ErrorCode::ReconciliationRequired,
                    format!(
                        "release {id} is damaged ({}) and so is its recovery copy ({})",
                        error.message, recovery.message
                    ),
                )
            })?;
            (copy, Some(error.message))
        }
    };
    let compatibility =
        repair_compatibility(&compiled(), &release.manifest.compatibility, leases.len()).map_err(
            |error| {
                Error::new(
                    error.code,
                    format!("release {id} cannot serve this journal: {}", error.message),
                )
            },
        )?;
    let closure = fs::symlink_metadata(paths.admission_marker()).is_ok();
    // A job left loaded, for example one that keeps failing, is replaced.
    manager.bootout(&options.label)?;
    let running = start(paths, manager, options, &release)?;
    if closure {
        reopen(paths, &admin_secret(paths)?)?;
    }
    let from_recovery = damaged.is_some();
    let replaced = selection.current.clone();
    selection.current = id.clone();
    selection.history.push(SelectionEvent {
        release_id: id.clone(),
        event: format!(
            "repaired from {replaced} to the last known good release{}",
            if from_recovery {
                ", from its recovery copy"
            } else {
                ""
            }
        ),
        unix_ms: unix_ms(),
    });
    write_selection(paths, &selection)?;
    Ok(RepairReport {
        release_id: id,
        artifact: release.dir.clone(),
        from_recovery,
        damaged,
        compatibility,
        running,
        reopened_closure: closure,
        selection,
    })
}

/// This build's compatibility, for checks that compare releases.
pub fn this_build() -> BuildCompatibility {
    compiled()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(capabilities: &[Capability]) -> BuildCompatibility {
        BuildCompatibility {
            capabilities: capabilities.iter().copied().collect(),
            ..compiled()
        }
    }

    #[test]
    fn a_replacement_must_read_the_journal_and_serve_every_consumer() {
        let current = compiled();
        let report = check_compatibility(&current, &current).unwrap();
        assert!(!report.downgrade && report.dropped.is_empty());
        // A release that states less is a downgrade, allowed while it reads
        // the same journal and serves the same consumers.
        let older = build(&[
            Capability::DurableAdmission,
            Capability::FencedLaunch,
            Capability::PerResourceEvidence,
            Capability::StaticControlReservations,
            Capability::MacosCooperative,
        ]);
        let report = check_compatibility(&current, &older).unwrap();
        assert!(report.downgrade);
        assert_eq!(
            report.dropped,
            BTreeSet::from([Capability::ParentLease, Capability::UpgradeDrain])
        );
        assert!(!check_compatibility(&older, &current).unwrap().downgrade);
        // Incompatible replacements are refused.
        for (to, needle) in [
            (
                BuildCompatibility {
                    journal_schema: "2".into(),
                    ..current.clone()
                },
                "journal schema",
            ),
            (
                BuildCompatibility {
                    protocol: current.protocol + 1,
                    ..current.clone()
                },
                "protocols",
            ),
            (
                BuildCompatibility {
                    wire_version: current.wire_version + 1,
                    ..current.clone()
                },
                "protocols",
            ),
            (
                BuildCompatibility {
                    config_schema: current.config_schema + 1,
                    ..current.clone()
                },
                "configuration",
            ),
            (build(&[Capability::DurableAdmission]), "consumers require"),
        ] {
            let error = check_compatibility(&current, &to).unwrap_err();
            assert_eq!(error.code, ErrorCode::ResourcePolicyUnsupported);
            assert!(error.message.contains(needle), "{needle}: {error:?}");
        }
    }

    #[test]
    fn repair_refuses_a_release_that_cannot_account_the_journals_leases() {
        let without_leases = build(&[
            Capability::DurableAdmission,
            Capability::FencedLaunch,
            Capability::MacosCooperative,
        ]);
        let report = repair_compatibility(&compiled(), &without_leases, 0).unwrap();
        assert!(report.downgrade && report.dropped.contains(&Capability::ParentLease));
        let error = repair_compatibility(&compiled(), &without_leases, 2).unwrap_err();
        assert_eq!(error.code, ErrorCode::ResourcePolicyUnsupported);
        assert!(error.message.contains("2 unreleased"), "{error:?}");
        assert!(repair_compatibility(&compiled(), &compiled(), 2).is_ok());
        let other_journal = BuildCompatibility {
            journal_schema: "0".into(),
            ..compiled()
        };
        assert!(repair_compatibility(&compiled(), &other_journal, 0).is_err());
    }

    #[test]
    fn release_ids_name_plain_directories() {
        let directory = tempfile::tempdir().unwrap();
        let paths = AuthorityPaths::fixture(directory.path());
        assert!(release_dir(&paths, "0.1.0-abc-12345678").is_ok());
        for bad in ["", ".x", "../state", "a/b", "a b"] {
            assert!(release_dir(&paths, bad).is_err(), "{bad}");
        }
    }
}
