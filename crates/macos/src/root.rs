//! Steps a scope root takes on itself before any payload runs: lead a new
//! process group and carry the utility QoS clamp. The authority verifies both
//! by readback; it never infers them from these calls having been made.

use devguard_contract::{Error, Result};
use std::ffi::OsString;
use std::path::Path;

/// Make the calling process the leader of a new process group, the observed
/// scope that later descendants inherit. A process that already leads its own
/// group, such as a session leader started on a pseudo-terminal, keeps it.
#[cfg(target_os = "macos")]
pub fn become_scope_root() -> Result<()> {
    // SAFETY: getpgrp, getpid and setpgid(0, 0) only read or change the calling
    // process's own group.
    unsafe {
        if libc::getpgrp() == libc::getpid() {
            return Ok(());
        }
        if libc::setpgid(0, 0) != 0 {
            return Err(crate::failed("cannot create the scope process group"));
        }
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn become_scope_root() -> Result<()> {
    Err(crate::unsupported())
}

/// Replace the current process image with `program` under the utility QoS
/// clamp (`POSIX_SPAWN_SETEXEC`), keeping the PID, process group, nice value,
/// environment and inherited descriptors. Returns only on failure.
#[cfg(target_os = "macos")]
pub fn exec_with_workload_qos(program: &Path, args: &[OsString]) -> Error {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let encode = |bytes: &[u8]| CString::new(bytes).ok();
    let Some(path) = encode(program.as_os_str().as_bytes()) else {
        return crate::failed("program path is not representable");
    };
    let mut argv: Vec<CString> = vec![path.clone()];
    for arg in args {
        match encode(arg.as_bytes()) {
            Some(arg) => argv.push(arg),
            None => return crate::failed("argument is not representable"),
        }
    }
    let mut envp: Vec<CString> = Vec::new();
    for (key, value) in std::env::vars_os() {
        let mut pair = key.as_bytes().to_vec();
        pair.push(b'=');
        pair.extend_from_slice(value.as_bytes());
        match CString::new(pair) {
            Ok(pair) => envp.push(pair),
            Err(_) => return crate::failed("environment is not representable"),
        }
    }
    let mut argv_ptrs: Vec<*mut libc::c_char> =
        argv.iter().map(|arg| arg.as_ptr().cast_mut()).collect();
    argv_ptrs.push(std::ptr::null_mut());
    let mut envp_ptrs: Vec<*mut libc::c_char> =
        envp.iter().map(|pair| pair.as_ptr().cast_mut()).collect();
    envp_ptrs.push(std::ptr::null_mut());
    // SAFETY: posix_spawnattr_t is initialized by posix_spawnattr_init below.
    let mut attributes: libc::posix_spawnattr_t = unsafe { std::mem::zeroed() };
    // SAFETY: all pointers reference live, NUL-terminated arrays built above;
    // with SETEXEC a successful call does not return.
    unsafe {
        if libc::posix_spawnattr_init(&mut attributes) != 0 {
            return crate::failed("cannot prepare spawn attributes");
        }
        if libc::posix_spawnattr_setflags(
            &mut attributes,
            libc::POSIX_SPAWN_SETEXEC as libc::c_short,
        ) == 0
            && crate::ffi::posix_spawnattr_set_qos_class_np(
                &mut attributes,
                crate::ffi::QOS_CLASS_UTILITY,
            ) == 0
        {
            let mut ignored: libc::pid_t = 0;
            libc::posix_spawn(
                &mut ignored,
                path.as_ptr(),
                std::ptr::null(),
                &attributes,
                argv_ptrs.as_ptr(),
                envp_ptrs.as_ptr(),
            );
        }
        libc::posix_spawnattr_destroy(&mut attributes);
    }
    crate::failed("cannot replace the process under the workload QoS clamp")
}

#[cfg(not(target_os = "macos"))]
pub fn exec_with_workload_qos(_: &Path, _: &[OsString]) -> Error {
    crate::unsupported()
}
