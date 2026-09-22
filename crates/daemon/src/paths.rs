use devguard_contract::{Error, ErrorCode, Result};
use serde::Serialize;
use std::ffi::CStr;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

/// Fields cannot be supplied by a caller or deserialized from project configuration.
#[derive(Debug, Clone, Serialize)]
pub struct AuthorityPaths {
    uid: u32,
    home: PathBuf,
    config_directory: PathBuf,
    root: PathBuf,
    runtime: PathBuf,
    cache: PathBuf,
}

impl AuthorityPaths {
    pub fn current_user() -> Result<Self> {
        if !cfg!(target_os = "macos") {
            return Err(Error::new(ErrorCode::ResourcePolicyUnsupported, "normal authority currently requires macOS; Linux CI covers portable contracts only"));
        }
        // SAFETY: these process credential observations do not dereference pointers.
        let uid = unsafe { libc::getuid() };
        if uid == 0 || uid != unsafe { libc::geteuid() } {
            return Err(Error::new(
                ErrorCode::Unauthorized,
                "run as the current unprivileged user",
            ));
        }
        // NSS account data, not HOME/XDG/socket overrides, determines normal ownership.
        let mut buffer = vec![0u8; 64 * 1024];
        // SAFETY: passwd is an output-only C struct initialized by getpwuid_r.
        let mut account: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        // SAFETY: all output pointers reference live, appropriately sized buffers.
        let result = unsafe {
            libc::getpwuid_r(
                uid,
                &mut account,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            )
        };
        if result != 0 || found.is_null() || account.pw_dir.is_null() {
            return Err(Error::new(
                ErrorCode::ResourceControlUnavailable,
                "cannot observe account home",
            ));
        }
        // SAFETY: successful getpwuid_r provides a terminated string in buffer.
        let home = PathBuf::from(std::ffi::OsStr::from_bytes(
            unsafe { CStr::from_ptr(account.pw_dir) }.to_bytes(),
        ));
        if !home.is_absolute() {
            return Err(invalid_path());
        }
        Ok(Self::for_account(
            uid,
            home,
            PathBuf::from(format!("/private/tmp/devguard-{uid}")),
        ))
    }

    fn for_account(uid: u32, home: PathBuf, runtime: PathBuf) -> Self {
        Self {
            uid,
            config_directory: home.join(".config/devguard"),
            root: home.join("Library/Application Support/DevGuard"),
            cache: home.join("Library/Caches/DevGuard"),
            home,
            runtime,
        }
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }
    pub fn config(&self) -> PathBuf {
        self.config_directory.join("host.toml")
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn runtime(&self) -> &Path {
        &self.runtime
    }
    pub fn state(&self) -> PathBuf {
        self.root.join("state")
    }
    pub fn journal(&self) -> PathBuf {
        self.state().join("authority.sqlite")
    }
    pub fn lock(&self) -> PathBuf {
        self.state().join("authority.lock")
    }
    pub fn socket(&self) -> PathBuf {
        self.runtime.join("authority.sock")
    }
    pub fn credentials(&self) -> PathBuf {
        self.root.join("credentials")
    }
    pub fn cli_credential(&self) -> PathBuf {
        self.credentials().join("dev-cli.secret")
    }
    pub fn admin_credential(&self) -> PathBuf {
        self.credentials().join("admin.secret")
    }

    /// Only explicit initialization may create persistent state/config directories.
    pub fn prepare_bootstrap(&self) -> Result<()> {
        for path in [
            &self.config_directory,
            &self.root,
            &self.state(),
            &self.credentials(),
        ] {
            secure_directory(path, self.uid, true)?;
        }
        Ok(())
    }

    pub fn validate_existing(&self) -> Result<()> {
        for path in [
            &self.config_directory,
            &self.root,
            &self.state(),
            &self.credentials(),
        ] {
            secure_directory(path, self.uid, false)?;
        }
        validate_private_file(&self.journal(), self.uid)?;
        validate_private_file(&self.lock(), self.uid)?;
        Ok(())
    }

    pub fn prepare_runtime(&self) -> Result<()> {
        secure_directory(&self.runtime, self.uid, true)
    }

    #[cfg(test)]
    pub(crate) fn fixture(base: &Path) -> Self {
        // Test-only fixture paths are not a production daemon argument or budget mode.
        Self::for_account(
            unsafe { libc::getuid() },
            base.join("home"),
            base.join("run"),
        )
    }
}

fn invalid_path() -> Error {
    Error::new(
        ErrorCode::Unauthorized,
        "authority path has an unsafe type, owner or permission",
    )
}

/// Walk each ancestor without following symlinks. Shared sticky tmp is permitted
/// only as an ancestor; every DevGuard leaf must be owned by this UID and 0700.
pub fn secure_directory(path: &Path, uid: u32, create: bool) -> Result<()> {
    if !path.is_absolute() {
        return Err(invalid_path());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir => current.push("/"),
            Component::Normal(name) => current.push(name),
            _ => return Err(invalid_path()),
        }
        match fs::symlink_metadata(&current) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                match DirBuilder::new().mode(0o700).create(&current) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(invalid_path()),
                }
            }
            Err(_) => return Err(invalid_path()),
        }
        let meta = fs::symlink_metadata(&current).map_err(|_| invalid_path())?;
        let shared_tmp = matches!(current.to_str(), Some("/private/tmp" | "/tmp"))
            && meta.uid() == 0
            && meta.mode() & 0o1000 != 0;
        if !meta.is_dir()
            || meta.file_type().is_symlink()
            || (meta.uid() != uid && meta.uid() != 0)
            || (!shared_tmp && meta.mode() & 0o022 != 0)
        {
            return Err(invalid_path());
        }
        if current == path && (meta.uid() != uid || meta.mode() & 0o077 != 0) {
            return Err(invalid_path());
        }
    }
    Ok(())
}

pub fn validate_private_file(path: &Path, uid: u32) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| invalid_path())?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || meta.uid() != uid
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
    {
        return Err(invalid_path());
    }
    Ok(())
}

pub fn read_private(path: &Path, uid: u32, limit: usize) -> Result<Vec<u8>> {
    validate_private_file(path, uid)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| invalid_path())?;
    let meta = file.metadata().map_err(|_| invalid_path())?;
    let named = fs::symlink_metadata(path).map_err(|_| invalid_path())?;
    if (meta.dev(), meta.ino()) != (named.dev(), named.ino())
        || meta.uid() != uid
        || meta.mode() & 0o077 != 0
        || !meta.is_file()
        || meta.nlink() != 1
    {
        return Err(invalid_path());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid_path())?;
    if bytes.len() > limit {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "private configuration exceeds its byte limit",
        ));
    }
    Ok(bytes)
}

pub fn write_new_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| {
            Error::new(
                ErrorCode::InvalidRequest,
                "bootstrap requires absent private files",
            )
        })?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| {
            Error::new(
                ErrorCode::JournalInvalid,
                "bootstrap write failed; preserve partial state for explicit repair",
            )
        })?;
    File::open(path.parent().ok_or_else(invalid_path)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::new(ErrorCode::JournalInvalid, "bootstrap directory sync failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    pub(crate) fn temporary() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("dg-path-")
            .tempdir_in(if cfg!(target_os = "macos") {
                "/private/tmp"
            } else {
                "/tmp"
            })
            .unwrap()
    }

    #[test]
    fn authority_paths_create_only_explicit_private_roots() {
        let dir = temporary();
        let paths = AuthorityPaths::fixture(dir.path());
        assert!(paths.validate_existing().is_err());
        assert!(!paths.root().exists());
        paths.prepare_bootstrap().unwrap();
        paths.prepare_runtime().unwrap();
        assert_eq!(fs::metadata(paths.root()).unwrap().mode() & 0o777, 0o700);
        assert!(!paths.journal().exists());
        assert_ne!(paths.state(), paths.runtime());
    }

    #[test]
    fn authority_paths_reject_aliases_and_do_not_fix_user_permissions() {
        let dir = temporary();
        let paths = AuthorityPaths::fixture(dir.path());
        paths.prepare_bootstrap().unwrap();
        let alias = dir.path().join("alias");
        symlink(paths.root(), &alias).unwrap();
        assert!(secure_directory(&alias, paths.uid(), false).is_err());
        fs::set_permissions(paths.root(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(paths.prepare_bootstrap().is_err());
        assert_eq!(fs::metadata(paths.root()).unwrap().mode() & 0o777, 0o755);
        assert!(secure_directory(&paths.root().join("../other"), paths.uid(), true).is_err());
    }

    #[test]
    fn authority_private_files_reject_links_oversize_and_shared_access() {
        let dir = temporary();
        let path = dir.path().join("secret");
        let uid = unsafe { libc::getuid() };
        write_new_private(&path, b"private").unwrap();
        assert_eq!(read_private(&path, uid, 7).unwrap(), b"private");
        assert!(read_private(&path, uid, 6).is_err());
        assert!(write_new_private(&path, b"replacement").is_err());
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(read_private(&link, uid, 10).is_err());
        fs::hard_link(&path, dir.path().join("hard")).unwrap();
        assert!(read_private(&path, uid, 10).is_err());
        fs::remove_file(dir.path().join("hard")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_private(&path, uid, 10).is_err());
        assert!(read_private(&path, uid.wrapping_add(1), 10).is_err());
    }
}
