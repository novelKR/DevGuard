//! Protected installation of a release as the current user's LaunchAgent.
//!
//! A package is a directory holding the three binaries and a manifest of their
//! hashes and compiled compatibility. The installer runs from the package's
//! own `devguardd`, so binaries from different builds are never mixed. It
//! copies the release into an immutable directory under the canonical root,
//! starts it through launchd with the same foreground `serve` entrypoint, and
//! only after verifying the running binary records it as current and last
//! known good and keeps a recovery copy. It never touches the journal: a
//! service that cannot open or reconcile its state fails closed, and launchd
//! restarts it only after a crash.

use crate::paths::{replace_private, secure_directory, AuthorityPaths};
use devguard_client::protocol::{AdmissionClosure, Hello, WIRE_VERSION};
use devguard_client::Client;
use devguard_contract::{
    digest_bytes, Capability, Compatibility, Error, ErrorCode, Result, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The current user's agent. A reverse-DNS name of the project's repository host.
pub const LABEL: &str = "io.github.novelkr.devguard";
pub const MANIFEST_SCHEMA: &str = "devguard-release-manifest/v1";
pub const SELECTION_SCHEMA: &str = "devguard-release-selection/v1";
/// The binaries a release consists of, all from one build.
pub const ARTIFACTS: [&str; 3] = ["devguardd", "devguard", "devguard-launch"];
const MANIFEST_FILE: &str = "MANIFEST.json";
const MANIFEST_BYTES: u64 = 64 * 1024;
const SELECTION_BYTES: usize = 256 * 1024;
const ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
const LAUNCHCTL: &str = "/bin/launchctl";

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidRequest, message)
}

fn unavailable(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceControlUnavailable, message)
}

/// What a build can serve and read, compiled in. A package states it, and the
/// installer refuses a package whose values differ from its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildCompatibility {
    pub package_version: String,
    pub wire_version: u32,
    pub protocol: u32,
    pub capabilities: BTreeSet<Capability>,
    pub journal_schema: String,
    pub config_schema: u32,
}

/// This build's compatibility.
pub fn compiled() -> BuildCompatibility {
    BuildCompatibility {
        package_version: env!("CARGO_PKG_VERSION").into(),
        wire_version: WIRE_VERSION,
        protocol: PROTOCOL_VERSION,
        capabilities: crate::server::launch_capabilities()
            .union(&crate::server::echoed_capabilities())
            .copied()
            .collect(),
        journal_schema: devguard_core::JOURNAL_SCHEMA.into(),
        config_schema: crate::config::CONFIG_SCHEMA,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub sha256: String,
    pub bytes: u64,
}

/// A release's manifest. `source` and `build` are provenance written by the
/// packaging tool; the installer records them without interpreting them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub release_id: String,
    pub version: String,
    pub source: serde_json::Value,
    pub build: serde_json::Value,
    pub artifacts: BTreeMap<String, Artifact>,
    pub compatibility: BuildCompatibility,
    /// `functional`: tested functionally, not measured for the SLO.
    pub scope: String,
    pub slo_qualified: bool,
    pub created_at: String,
}

/// A validated package.
#[derive(Debug, Clone)]
pub struct Package {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// The digest of the manifest's exact bytes; it identifies the release.
    pub manifest_sha256: String,
}

fn valid_release_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Read a regular file that is not a symlink, bounded by `limit`.
fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| invalid(format!("{} is missing", path.display())))?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(invalid(format!("{} is not a regular file", path.display())));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| invalid(format!("cannot read {}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid(format!("cannot read {}", path.display())))?;
    if bytes.len() as u64 > limit {
        return Err(invalid(format!(
            "{} exceeds its size limit",
            path.display()
        )));
    }
    Ok(bytes)
}

pub(crate) fn digest_file(path: &Path) -> Result<String> {
    Ok(digest_bytes(&read_regular(path, ARTIFACT_BYTES)?))
}

fn directory_entries(path: &Path) -> Result<BTreeSet<String>> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| invalid(format!("{} is missing", path.display())))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(invalid(format!("{} is not a directory", path.display())));
    }
    fs::read_dir(path)
        .map_err(|_| invalid(format!("cannot list {}", path.display())))?
        .map(|entry| {
            entry
                .map_err(|_| invalid("cannot list a package directory"))?
                .file_name()
                .into_string()
                .map_err(|_| invalid("package entries must be UTF-8"))
        })
        .collect()
}

/// Check a package or an installed release: exactly the manifest and the three
/// binaries, each a regular executable file whose size and hash match, and a
/// manifest this build can install.
pub fn validate_package(dir: &Path) -> Result<Package> {
    let package = read_release(dir)?;
    let expected = compiled();
    if package.manifest.compatibility != expected
        || package.manifest.version != expected.package_version
    {
        return Err(Error::new(
            ErrorCode::ResourcePolicyUnsupported,
            "the package's compatibility differs from this installer's build",
        ));
    }
    Ok(package)
}

/// Check an intact release of any build: exactly the manifest and the three
/// binaries, each a regular executable file whose size and hash match. Its
/// compatibility may differ from this build's, as an earlier release's does.
pub fn read_release(dir: &Path) -> Result<Package> {
    let entries = directory_entries(dir)?;
    if entries != BTreeSet::from([MANIFEST_FILE.to_string(), "bin".to_string()]) {
        return Err(invalid("a package holds exactly MANIFEST.json and bin/"));
    }
    let bytes = read_regular(&dir.join(MANIFEST_FILE), MANIFEST_BYTES)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("the package manifest is not a valid release manifest"))?;
    if manifest.schema != MANIFEST_SCHEMA {
        return Err(invalid("unsupported release manifest schema"));
    }
    if !valid_release_id(&manifest.release_id) {
        return Err(invalid(
            "the release id may use only letters, digits, '.', '_' and '-'",
        ));
    }
    if !matches!(
        (manifest.scope.as_str(), manifest.slo_qualified),
        ("functional", false) | ("slo-qualified", true)
    ) {
        return Err(invalid("the manifest scope and SLO qualification disagree"));
    }
    let names: BTreeSet<&str> = manifest.artifacts.keys().map(String::as_str).collect();
    if names != BTreeSet::from(ARTIFACTS) {
        return Err(invalid(
            "the manifest must list exactly devguardd, devguard and devguard-launch",
        ));
    }
    let bin = dir.join("bin");
    if directory_entries(&bin)? != ARTIFACTS.iter().map(|name| name.to_string()).collect() {
        return Err(invalid("bin/ holds exactly the three release binaries"));
    }
    for (name, artifact) in &manifest.artifacts {
        let path = bin.join(name);
        let meta =
            fs::symlink_metadata(&path).map_err(|_| invalid("a release binary is missing"))?;
        if !meta.is_file() || meta.permissions().mode() & 0o100 == 0 {
            return Err(invalid(format!("{name} is not an executable regular file")));
        }
        if meta.len() != artifact.bytes || digest_file(&path)? != artifact.sha256 {
            return Err(invalid(format!("{name} does not match its manifest hash")));
        }
    }
    Ok(Package {
        dir: dir.to_path_buf(),
        manifest,
        manifest_sha256: digest_bytes(&bytes),
    })
}

/// How the service is registered with launchd. The installed service always
/// uses [`InstallOptions::canonical`]; tests use a unique label and paths.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub label: String,
    pub plist: PathBuf,
    pub log: PathBuf,
    /// Arguments after the release's `devguardd`: `serve` for the service.
    pub program_args: Vec<String>,
    pub environment: BTreeMap<String, String>,
    /// How long the started service may take to prove it is the release.
    pub start_deadline: Duration,
    /// Seconds launchd waits before restarting a crashed service.
    pub throttle_seconds: u32,
}

impl InstallOptions {
    pub fn canonical(paths: &AuthorityPaths) -> Self {
        Self {
            label: LABEL.into(),
            plist: paths.launch_agents().join(format!("{LABEL}.plist")),
            log: paths.logs().join("devguardd.log"),
            program_args: vec!["serve".into()],
            environment: BTreeMap::new(),
            start_deadline: Duration::from_secs(15),
            throttle_seconds: 10,
        }
    }
}

/// What launchd is asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub label: String,
    pub plist: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

/// The service's state as launchd reports it.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ServiceState {
    pub running: bool,
    pub pid: Option<u32>,
    pub runs: Option<u64>,
    pub last_exit: Option<String>,
}

/// The seam to launchd. [`Launchctl`] is the real one.
pub trait ServiceManager {
    fn bootstrap(&self, spec: &ServiceSpec) -> Result<()>;
    fn bootout(&self, label: &str) -> Result<()>;
    /// `None` when no job with the label is loaded.
    fn state(&self, label: &str) -> Result<Option<ServiceState>>;
}

/// The current user's GUI launchd domain.
pub struct Launchctl {
    domain: String,
}

impl Launchctl {
    pub fn new(uid: u32) -> Self {
        Self {
            domain: format!("gui/{uid}"),
        }
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output> {
        use std::os::unix::process::CommandExt;
        // Its own process group: a terminal's interrupt meant for the
        // operator's command must not fail a bootout or bootstrap midway.
        std::process::Command::new(LAUNCHCTL)
            .args(args)
            .process_group(0)
            .output()
            .map_err(|_| unavailable("cannot run launchctl"))
    }
}

/// launchctl's exit status when `print` cannot find a service.
const LAUNCHCTL_NO_SUCH_SERVICE: i32 = 113;
/// launchctl's exit status (ESRCH) when `bootout` finds no such service.
const LAUNCHCTL_NO_SUCH_PROCESS: i32 = 3;

impl ServiceManager for Launchctl {
    fn bootstrap(&self, spec: &ServiceSpec) -> Result<()> {
        let plist = spec
            .plist
            .to_str()
            .ok_or_else(|| invalid("the plist path must be UTF-8"))?;
        let output = self.run(&["bootstrap", &self.domain, plist])?;
        if !output.status.success() {
            return Err(unavailable(format!(
                "launchctl bootstrap failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    /// Booting out a service that is not loaded succeeds: it is already out.
    fn bootout(&self, label: &str) -> Result<()> {
        if self.state(label)?.is_none() {
            return Ok(());
        }
        let output = self.run(&["bootout", &format!("{}/{label}", self.domain)])?;
        let gone = matches!(
            output.status.code(),
            Some(LAUNCHCTL_NO_SUCH_SERVICE | LAUNCHCTL_NO_SUCH_PROCESS)
        );
        if !output.status.success() && !gone {
            return Err(unavailable(format!(
                "launchctl bootout failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    }

    fn state(&self, label: &str) -> Result<Option<ServiceState>> {
        let output = self.run(&["print", &format!("{}/{label}", self.domain)])?;
        if output.status.code() == Some(LAUNCHCTL_NO_SUCH_SERVICE) {
            return Ok(None);
        }
        if !output.status.success() {
            return Err(unavailable("launchctl print failed"));
        }
        Ok(Some(parse_print(&String::from_utf8_lossy(&output.stdout))))
    }
}

/// Read the service's own fields from `launchctl print`: only lines indented
/// by exactly one tab belong to the service rather than a nested section.
pub fn parse_print(text: &str) -> ServiceState {
    let mut state = ServiceState::default();
    for line in text.lines() {
        let Some(line) = line.strip_prefix('\t') else {
            continue;
        };
        if line.starts_with('\t') {
            continue;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        match key {
            "state" => state.running = value == "running",
            "pid" => state.pid = value.parse().ok(),
            "runs" => state.runs = value.parse().ok(),
            "last exit code" | "last terminating signal" => {
                state.last_exit = Some(format!("{key} = {value}"))
            }
            _ => {}
        }
    }
    if !state.running {
        state.pid = None;
    }
    state
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The agent's property list. A crash restarts the service; a clean or failing
/// exit, such as a fail-closed start on a missing journal, does not.
pub fn render_plist(spec: &ServiceSpec, log: &Path, throttle_seconds: u32) -> Result<String> {
    let text = |path: &Path| {
        path.to_str()
            .map(xml_escape)
            .ok_or_else(|| invalid("service paths must be UTF-8"))
    };
    let mut arguments = format!("    <string>{}</string>\n", text(&spec.program)?);
    for arg in &spec.args {
        arguments.push_str(&format!("    <string>{}</string>\n", xml_escape(arg)));
    }
    let mut environment = String::new();
    if !spec.environment.is_empty() {
        environment.push_str("  <key>EnvironmentVariables</key>\n  <dict>\n");
        for (name, value) in &spec.environment {
            environment.push_str(&format!(
                "    <key>{}</key>\n    <string>{}</string>\n",
                xml_escape(name),
                xml_escape(value)
            ));
        }
        environment.push_str("  </dict>\n");
    }
    let log = text(log)?;
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{arguments}  </array>
{environment}  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>Crashed</key>
    <true/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>{throttle_seconds}</integer>
  <key>ProcessType</key>
  <string>Standard</string>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
",
        label = xml_escape(&spec.label),
    ))
}

/// Which release the service runs and which last ran verified.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub schema: String,
    pub current: String,
    pub last_known_good: Option<String>,
    pub history: Vec<SelectionEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SelectionEvent {
    pub release_id: String,
    pub event: String,
    pub unix_ms: u128,
}

pub(crate) fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or(0)
}

pub fn read_selection(paths: &AuthorityPaths) -> Result<Option<Selection>> {
    let path = paths.selection();
    if fs::symlink_metadata(&path).is_err() {
        return Ok(None);
    }
    let bytes = crate::paths::read_private(&path, paths.uid(), SELECTION_BYTES)?;
    let selection: Selection = serde_json::from_slice(&bytes).map_err(|_| {
        Error::new(
            ErrorCode::JournalInvalid,
            "the release selection is not valid",
        )
    })?;
    if selection.schema != SELECTION_SCHEMA {
        return Err(Error::new(
            ErrorCode::JournalInvalid,
            "unsupported release selection schema",
        ));
    }
    Ok(Some(selection))
}

pub(crate) fn write_selection(paths: &AuthorityPaths, selection: &Selection) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(selection).map_err(|_| invalid("selection encoding failed"))?;
    replace_private(&paths.selection(), &bytes)
}

/// Copy `from` (a validated package or release) into `destination` as an
/// immutable release: the copy is made in a private sibling, synced, made
/// read-only and renamed into place, so a partial copy never has the final
/// name. An existing destination is reused only when its manifest is identical.
pub(crate) fn freeze_copy(from: &Package, destination: &Path) -> Result<bool> {
    if fs::symlink_metadata(destination).is_ok() {
        let existing = validate_package(destination)?;
        if existing.manifest_sha256 != from.manifest_sha256 {
            return Err(invalid(format!(
                "{} holds a different release; releases are never overwritten",
                destination.display()
            )));
        }
        return Ok(true);
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("a release needs a parent"))?;
    let incoming = parent.join(format!(
        ".incoming-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    let copied = (|| -> std::io::Result<()> {
        fs::DirBuilder::new().mode(0o700).create(&incoming)?;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(incoming.join("bin"))?;
        for name in ARTIFACTS {
            let target = incoming.join("bin").join(name);
            fs::copy(from.dir.join("bin").join(name), &target)?;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o500))?;
            File::open(&target)?.sync_all()?;
        }
        let manifest = incoming.join(MANIFEST_FILE);
        fs::copy(from.dir.join(MANIFEST_FILE), &manifest)?;
        fs::set_permissions(&manifest, fs::Permissions::from_mode(0o400))?;
        File::open(&manifest)?.sync_all()?;
        File::open(incoming.join("bin"))?.sync_all()?;
        fs::set_permissions(incoming.join("bin"), fs::Permissions::from_mode(0o500))?;
        Ok(())
    })();
    if copied.is_err() {
        remove_incoming(&incoming);
        return Err(unavailable(format!(
            "cannot copy the release into {}",
            parent.display()
        )));
    }
    let copy = validate_package(&incoming);
    let copy = match copy {
        Ok(copy) if copy.manifest_sha256 == from.manifest_sha256 => copy,
        _ => {
            remove_incoming(&incoming);
            return Err(unavailable("the copied release does not match its package"));
        }
    };
    drop(copy);
    if fs::rename(&incoming, destination).is_err() {
        remove_incoming(&incoming);
        // Another installer may have frozen the same release first.
        let existing = validate_package(destination)?;
        if existing.manifest_sha256 != from.manifest_sha256 {
            return Err(invalid("a different release took this release's name"));
        }
        return Ok(true);
    }
    let _ = fs::set_permissions(destination, fs::Permissions::from_mode(0o500));
    let _ = File::open(parent).and_then(|dir| dir.sync_all());
    Ok(false)
}

/// Remove an incoming copy, which may already be partly read-only.
fn remove_incoming(incoming: &Path) {
    let _ = fs::set_permissions(incoming, fs::Permissions::from_mode(0o700));
    let _ = fs::set_permissions(incoming.join("bin"), fs::Permissions::from_mode(0o700));
    let _ = fs::remove_dir_all(incoming);
}

/// Remove incoming copies left by installers that are no longer running.
pub(crate) fn sweep_incoming(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(".incoming-") else {
            continue;
        };
        let pid: Option<i32> = rest.split('-').next().and_then(|pid| pid.parse().ok());
        // SAFETY: signal 0 only checks whether the process exists.
        let alive = pid.is_some_and(|pid| unsafe { libc::kill(pid, 0) } == 0);
        if !alive {
            remove_incoming(&entry.path());
        }
    }
}

/// A handshake that demands nothing, to learn who serves the socket.
pub(crate) fn handshake(paths: &AuthorityPaths) -> Result<Hello> {
    let client = Client::connect(
        &paths.socket(),
        paths.uid(),
        Compatibility {
            minimum_protocol: PROTOCOL_VERSION,
            maximum_protocol: PROTOCOL_VERSION,
            required: BTreeSet::new(),
        },
    )?;
    Ok(client.hello.clone())
}

/// What the installed service was observed running.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunningEvidence {
    pub service_pid: u32,
    pub authority_pid: u32,
    pub executable: PathBuf,
    pub sha256: String,
    pub capabilities: BTreeSet<Capability>,
}

/// Prove that the service launchd runs is `release`'s `devguardd`, serving
/// the canonical endpoint.
pub(crate) fn observe_running(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    label: &str,
    release: &Package,
) -> Result<RunningEvidence> {
    let state = manager
        .state(label)?
        .ok_or_else(|| unavailable("the service is not loaded"))?;
    let service_pid = state
        .pid
        .filter(|_| state.running)
        .ok_or_else(|| unavailable("the service is not running"))?;
    let hello = handshake(paths)?;
    if hello.authority.pid != service_pid {
        return Err(unavailable("another process serves the authority endpoint"));
    }
    let executable = devguard_macos::executable_path(service_pid)?
        .ok_or_else(|| unavailable("the service ended during verification"))?;
    let expected = release.dir.join("bin/devguardd");
    let same = fs::canonicalize(&executable).ok() == fs::canonicalize(&expected).ok();
    let sha256 = digest_file(&executable)?;
    if !same || sha256 != release.manifest.artifacts["devguardd"].sha256 {
        return Err(unavailable(
            "the running service is not the installed release",
        ));
    }
    Ok(RunningEvidence {
        service_pid,
        authority_pid: hello.authority.pid,
        executable,
        sha256,
        capabilities: hello.capabilities,
    })
}

pub(crate) fn wait_running(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
    release: &Package,
) -> Result<RunningEvidence> {
    let deadline = Instant::now() + options.start_deadline;
    loop {
        match observe_running(paths, manager, &options.label, release) {
            Ok(evidence) => return Ok(evidence),
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub release_id: String,
    pub release: PathBuf,
    pub manifest_sha256: String,
    /// An identical release was already installed and was reused.
    pub reused_release: bool,
    pub label: String,
    pub plist: PathBuf,
    pub running: RunningEvidence,
    pub recovery: PathBuf,
    pub selection: Selection,
}

pub(crate) fn ensure_plain_directory(path: &Path, mode: u32) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(invalid(format!("{} is not a directory", path.display()))),
        Err(_) => fs::DirBuilder::new()
            .recursive(true)
            .mode(mode)
            .create(path)
            .map_err(|_| unavailable(format!("cannot create {}", path.display()))),
    }
}

/// Install `package` as the service: refuse while another authority or an
/// agent exists, freeze the release, start it through `manager`, verify the
/// running binary, then record the selection and a recovery copy.
pub fn install(
    paths: &AuthorityPaths,
    package: &Path,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
) -> Result<InstallReport> {
    if !cfg!(target_os = "macos") {
        return Err(Error::new(
            ErrorCode::ResourcePolicyUnsupported,
            "the service is installed as a macOS LaunchAgent",
        ));
    }
    let package = validate_package(package)?;
    let running =
        std::env::current_exe().map_err(|_| unavailable("cannot locate the running installer"))?;
    if digest_file(&running)? != package.manifest.artifacts["devguardd"].sha256 {
        return Err(invalid(
            "run the installer from the package it installs: its devguardd must be this binary",
        ));
    }
    // The service needs the existing state; installation never creates or repairs it.
    paths.validate_existing()?;
    let _operation = crate::upgrade::operation(paths)?;
    if handshake(paths).is_ok() {
        return Err(Error::new(
            ErrorCode::ResourceUnavailable,
            "an authority is serving; stop it before installing the service",
        ));
    }
    drop(
        devguard_core::AuthorityStorage::open(&paths.journal()).map_err(|_| {
            Error::new(
                ErrorCode::ResourceUnavailable,
                "the authority state is held or invalid; installation needs it idle and valid",
            )
        })?,
    );
    if manager.state(&options.label)?.is_some() || fs::symlink_metadata(&options.plist).is_ok() {
        return Err(Error::new(
            ErrorCode::ResourceUnavailable,
            "the service is already installed; replacing it is an upgrade",
        ));
    }
    secure_directory(&paths.releases(), paths.uid(), true)?;
    secure_directory(&paths.recovery(), paths.uid(), true)?;
    let log_directory = options
        .log
        .parent()
        .ok_or_else(|| invalid("the log needs a directory"))?;
    secure_directory(log_directory, paths.uid(), true)?;
    let agents = options
        .plist
        .parent()
        .ok_or_else(|| invalid("the plist needs a directory"))?;
    ensure_plain_directory(agents, 0o755)?;
    sweep_incoming(&paths.releases());
    sweep_incoming(&paths.recovery());
    let id = package.manifest.release_id.clone();
    let release_dir = paths.releases().join(&id);
    let reused = freeze_copy(&package, &release_dir)?;
    let release = validate_package(&release_dir)?;
    let spec = ServiceSpec {
        label: options.label.clone(),
        plist: options.plist.clone(),
        program: release_dir.join("bin/devguardd"),
        args: options.program_args.clone(),
        environment: options.environment.clone(),
    };
    let plist = render_plist(&spec, &options.log, options.throttle_seconds)?;
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&options.plist)
        .and_then(|mut file| {
            file.write_all(plist.as_bytes())?;
            file.sync_all()
        });
    if written.is_err() {
        return Err(Error::new(
            ErrorCode::ResourceUnavailable,
            "the service plist already exists or cannot be written",
        ));
    }
    let started = manager
        .bootstrap(&spec)
        .and_then(|()| wait_running(paths, manager, options, &release));
    let evidence = match started {
        Ok(evidence) => evidence,
        Err(error) => {
            // Nothing was selected: stop and unload the unverified service.
            let _ = manager.bootout(&options.label);
            let _ = fs::remove_file(&options.plist);
            return Err(error);
        }
    };
    // A service that nothing selects must not keep running: if recording it
    // fails, it is stopped and unloaded again.
    let recovery = paths.recovery().join(&id);
    let recorded = freeze_copy(&release, &recovery).and_then(|_| {
        let mut selection = read_selection(paths)?.unwrap_or(Selection {
            schema: SELECTION_SCHEMA.into(),
            current: id.clone(),
            last_known_good: None,
            history: Vec::new(),
        });
        selection.current = id.clone();
        selection.last_known_good = Some(id.clone());
        selection.history.push(SelectionEvent {
            release_id: id.clone(),
            event: "installed and verified running".into(),
            unix_ms: unix_ms(),
        });
        write_selection(paths, &selection)?;
        Ok(selection)
    });
    let selection = match recorded {
        Ok(selection) => selection,
        Err(error) => {
            let _ = manager.bootout(&options.label);
            let _ = fs::remove_file(&options.plist);
            // Let the unselected service go before anyone tries again.
            let _ = crate::upgrade::released(paths);
            return Err(error);
        }
    };
    Ok(InstallReport {
        release_id: id,
        release: release_dir,
        manifest_sha256: release.manifest_sha256,
        reused_release: reused,
        label: options.label.clone(),
        plist: options.plist.clone(),
        running: evidence,
        recovery,
        selection,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    pub label: String,
    pub plist: PathBuf,
    /// The plist is exactly what this build renders for the current release.
    pub plist_matches_selection: bool,
    pub service: Option<ServiceState>,
    pub selection: Option<Selection>,
    pub running: Option<RunningEvidence>,
    pub running_error: Option<String>,
    pub releases: Vec<String>,
    pub recovery: Vec<String>,
    /// Admission closed by an upgrade or an administrator, on a release that
    /// honours the closure.
    pub admission_closed: Option<AdmissionClosure>,
    /// The service runs the verified current release with admission open.
    pub healthy: bool,
}

fn listing(path: &Path) -> Vec<String> {
    fs::read_dir(path)
        .map(|entries| {
            let mut names: Vec<String> = entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| !name.starts_with('.') && !name.ends_with(".json"))
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default()
}

/// Report the installed service without changing anything.
pub fn status(
    paths: &AuthorityPaths,
    manager: &dyn ServiceManager,
    options: &InstallOptions,
) -> Result<StatusReport> {
    let selection = read_selection(paths)?;
    let service = manager.state(&options.label)?;
    let mut report = StatusReport {
        label: options.label.clone(),
        plist: options.plist.clone(),
        plist_matches_selection: false,
        service,
        selection: selection.clone(),
        running: None,
        running_error: None,
        releases: listing(&paths.releases()),
        recovery: listing(&paths.recovery()),
        admission_closed: None,
        healthy: false,
    };
    let Some(selection) = selection else {
        report.running_error = Some("no release has been installed".into());
        return Ok(report);
    };
    // The service runs the current release or, after a repair, its recovery
    // copy; either must be intact, and the plist must name the one that runs.
    let installed = fs::read_to_string(&options.plist).ok();
    let mut chosen = None;
    let mut damage = Vec::new();
    for dir in [
        paths.releases().join(&selection.current),
        paths.recovery().join(&selection.current),
    ] {
        let release = match read_release(&dir) {
            Ok(release) => release,
            Err(error) => {
                damage.push(format!("{}: {}", dir.display(), error.message));
                continue;
            }
        };
        let spec = ServiceSpec {
            label: options.label.clone(),
            plist: options.plist.clone(),
            program: dir.join("bin/devguardd"),
            args: options.program_args.clone(),
            environment: options.environment.clone(),
        };
        let matches = render_plist(&spec, &options.log, options.throttle_seconds)
            .ok()
            .zip(installed.as_ref())
            .is_some_and(|(rendered, actual)| rendered == *actual);
        if matches || chosen.is_none() {
            chosen = Some((release, matches));
        }
        if matches {
            break;
        }
    }
    let Some((release, matches)) = chosen else {
        report.running_error = Some(format!(
            "the current release is not intact: {}",
            damage.join("; ")
        ));
        return Ok(report);
    };
    report.plist_matches_selection = matches;
    match observe_running(paths, manager, &options.label, &release) {
        Ok(evidence) => report.running = Some(evidence),
        Err(error) => report.running_error = Some(error.message),
    }
    // A release before C11 reads no marker, so one left behind closes nothing.
    if release
        .manifest
        .compatibility
        .capabilities
        .contains(&Capability::UpgradeDrain)
        && fs::symlink_metadata(paths.admission_marker()).is_ok()
    {
        report.admission_closed = Some(
            crate::paths::read_private(&paths.admission_marker(), paths.uid(), 4096)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or(AdmissionClosure {
                    reason: "the admission marker is unreadable".into(),
                    since_unix_ms: 0,
                }),
        );
    }
    report.healthy = report.plist_matches_selection
        && report.running.is_some()
        && report.admission_closed.is_none();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchctl_print_is_read_from_the_service_level_only() {
        let text = "gui/501/io.example = {\n\tactive count = 1\n\tpath = /x.plist\n\tstate = running\n\n\tprogram = /bin/sh\n\truns = 2\n\tpid = 4242\n\tlast terminating signal = Segmentation fault: 11\n\tendpoints = {\n\t\tstate = active\n\t\tpid = 7\n\t}\n}\n";
        let state = parse_print(text);
        assert!(state.running);
        assert_eq!(state.pid, Some(4242));
        assert_eq!(state.runs, Some(2));
        assert_eq!(
            state.last_exit.as_deref(),
            Some("last terminating signal = Segmentation fault: 11")
        );
        let stopped =
            parse_print("x = {\n\tstate = not running\n\truns = 3\n\tlast exit code = 1\n}\n");
        assert!(!stopped.running);
        assert_eq!(stopped.pid, None);
        assert_eq!(stopped.last_exit.as_deref(), Some("last exit code = 1"));
    }

    #[test]
    fn the_plist_restarts_only_a_crashed_service_and_escapes_its_values() {
        let spec = ServiceSpec {
            label: LABEL.into(),
            plist: "/Users/u/Library/LaunchAgents/x.plist".into(),
            program: "/Users/u/Library/Application Support/DevGuard/releases/r&1/bin/devguardd"
                .into(),
            args: vec!["serve".into()],
            environment: BTreeMap::from([("A".to_string(), "<b>".to_string())]),
        };
        let plist = render_plist(
            &spec,
            Path::new("/Users/u/Library/Logs/DevGuard/devguardd.log"),
            10,
        )
        .unwrap();
        assert!(plist.contains("<string>/Users/u/Library/Application Support/DevGuard/releases/r&amp;1/bin/devguardd</string>\n    <string>serve</string>"));
        assert!(
            plist.contains("<key>KeepAlive</key>\n  <dict>\n    <key>Crashed</key>\n    <true/>")
        );
        assert!(plist.contains("<key>A</key>\n    <string>&lt;b&gt;</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n  <true/>"));
        let canonical = render_plist(
            &ServiceSpec {
                environment: BTreeMap::new(),
                ..spec
            },
            Path::new("/l"),
            10,
        )
        .unwrap();
        assert!(!canonical.contains("EnvironmentVariables"));
    }

    #[test]
    fn release_ids_are_plain_names() {
        for good in ["0.1.0-d30fbce-1a2b3c4d", "r_1", "a.b-c"] {
            assert!(valid_release_id(good), "{good}");
        }
        for bad in ["", ".hidden", "a/b", "../x", "a b", &"x".repeat(129)] {
            assert!(!valid_release_id(bad), "{bad}");
        }
    }

    #[test]
    fn this_build_states_its_own_compatibility() {
        let build = compiled();
        assert_eq!(build.package_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(build.journal_schema, devguard_core::JOURNAL_SCHEMA);
        assert!(build.capabilities.contains(&Capability::FencedLaunch));
    }
}
