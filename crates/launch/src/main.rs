//! `devguard-launch`: the helper between an owner's launch grant and the
//! user's executable. It leads its own process group, carries the utility QoS
//! clamp, presents the one-time grant to the authority and reports READY only
//! after the authority has bound its scope and authorized the run. It never
//! attempts the executable without that authorization.
//!
//! The owner builds the invocation with `devguard_client::launch::helper_command`:
//!
//! ```text
//! devguard-launch --endpoint PATH --consumer ID --generation ID --attempt ID
//!     --instance ID --permit-fd N --report-fd N -- PROGRAM [ARGS...]
//! ```

use devguard_client::credential::take_inherited;
use devguard_client::launch::{
    HelperPhase, EXEC_FAILED_STATUS, NOT_AUTHORIZED_STATUS, NOT_FOUND_STATUS,
};
use devguard_client::Client;
use devguard_contract::{validate_id, AttemptKey, Capability, Compatibility, ErrorCode};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::fd::RawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// Marks the re-execution that already carries the utility QoS clamp.
const CLAMPED_STAGE: &str = "clamped";
/// A report longer than this is truncated, so every line is one atomic write.
const MESSAGE_BYTES: usize = 256;

struct Arguments {
    endpoint: PathBuf,
    key: AttemptKey,
    instance_id: String,
    permit_fd: RawFd,
    report_fd: RawFd,
    clamped: bool,
    program: PathBuf,
    args: Vec<OsString>,
    /// The helper's own options, repeated for the clamped re-execution.
    options: Vec<OsString>,
}

fn text(value: OsString) -> Option<String> {
    value.into_string().ok()
}

fn descriptor(value: OsString) -> Option<RawFd> {
    text(value)?.parse().ok().filter(|fd| *fd > 2)
}

fn parse() -> Option<Arguments> {
    let mut raw = std::env::args_os().skip(1);
    let mut options = Vec::new();
    let (mut endpoint, mut consumer, mut generation, mut attempt) = (None, None, None, None);
    let (mut instance, mut permit_fd, mut report_fd, mut clamped) = (None, None, None, false);
    loop {
        let option = raw.next()?;
        if option == "--" {
            break;
        }
        let value = raw.next()?;
        options.push(option.clone());
        options.push(value.clone());
        match option.to_str()? {
            "--endpoint" => endpoint = Some(PathBuf::from(value)),
            "--consumer" => consumer = text(value),
            "--generation" => generation = text(value),
            "--attempt" => attempt = text(value),
            "--instance" => instance = text(value),
            "--permit-fd" => permit_fd = descriptor(value),
            "--report-fd" => report_fd = descriptor(value),
            "--stage" if value == CLAMPED_STAGE => {
                // The stage marker is added by the helper, never repeated.
                options.truncate(options.len() - 2);
                clamped = true;
            }
            _ => return None,
        }
    }
    let program = PathBuf::from(raw.next()?);
    let key = AttemptKey {
        consumer_id: consumer?,
        consumer_generation: generation?,
        attempt_id: attempt?,
    };
    let instance_id = instance?;
    let endpoint = endpoint?;
    if key.validate().is_err()
        || validate_id(&instance_id).is_err()
        || !endpoint.is_absolute()
        || !program.is_absolute()
    {
        return None;
    }
    let (permit_fd, report_fd) = (permit_fd?, report_fd?);
    if permit_fd == report_fd {
        return None;
    }
    Some(Arguments {
        endpoint,
        key,
        instance_id,
        permit_fd,
        report_fd,
        clamped,
        program,
        args: raw.collect(),
        options,
    })
}

/// Write one transcript line. The owner may already be gone; a failed write
/// changes nothing about what the helper may do next.
fn report(fd: RawFd, phase: HelperPhase) {
    let phase = match phase {
        HelperPhase::Failed { stage, message } => HelperPhase::Failed {
            stage,
            message: bounded(message),
        },
        HelperPhase::Refused { code, message } => HelperPhase::Refused {
            code,
            message: bounded(message),
        },
        other => other,
    };
    let Ok(mut line) = serde_json::to_vec(&phase) else {
        return;
    };
    line.push(b'\n');
    let mut written = 0;
    while written < line.len() {
        // SAFETY: the slice is live for the call; fd is the inherited report
        // descriptor, and a closed or invalid descriptor only fails the write.
        let result =
            unsafe { libc::write(fd, line[written..].as_ptr().cast(), line.len() - written) };
        if result > 0 {
            written += result as usize;
        } else if result < 0
            && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
        {
            continue;
        } else {
            return;
        }
    }
}

fn bounded(mut message: String) -> String {
    if message.len() > MESSAGE_BYTES {
        let mut end = MESSAGE_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    message
}

fn failed(fd: RawFd, stage: &str, message: impl Into<String>) -> i32 {
    report(
        fd,
        HelperPhase::Failed {
            stage: stage.into(),
            message: message.into(),
        },
    );
    NOT_AUTHORIZED_STATUS
}

/// Lead a process group and re-execute under the utility QoS clamp, keeping
/// the PID and the two private descriptors. Returns only on failure.
fn clamp(arguments: &Arguments) -> i32 {
    if let Err(error) = devguard_macos::become_scope_root() {
        return failed(arguments.report_fd, "scope", error.message);
    }
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return failed(arguments.report_fd, "clamp", "cannot locate the helper"),
    };
    let mut again = arguments.options.clone();
    again.extend(["--stage".into(), CLAMPED_STAGE.into(), "--".into()]);
    again.push(arguments.program.clone().into_os_string());
    again.extend(arguments.args.iter().cloned());
    let error = devguard_macos::exec_with_workload_qos(&executable, &again);
    failed(arguments.report_fd, "clamp", error.message)
}

/// Present the grant, and exec only on the first successful authorization.
fn authorized_exec(arguments: Arguments) -> i32 {
    let fd = arguments.report_fd;
    // The permit descriptor is consumed and closed on every path.
    // SAFETY: the owner passed this descriptor to this process for the grant only.
    let permit = unsafe { take_inherited(arguments.permit_fd) };
    // The executable must not inherit the transcript.
    // SAFETY: fcntl only changes the flags of the inherited report descriptor.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
        return failed(fd, "report", "cannot protect the report descriptor");
    }
    let permit = match permit {
        Ok(permit) => permit,
        Err(error) => return failed(fd, "permit", error.message),
    };
    let compatibility = Compatibility {
        minimum_protocol: 1,
        maximum_protocol: 1,
        required: BTreeSet::from([Capability::FencedLaunch]),
    };
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    let mut client = match Client::connect(&arguments.endpoint, uid, compatibility) {
        Ok(client) => client,
        Err(error) => return failed(fd, "connect", error.message),
    };
    let authorization = client.launch(arguments.key, arguments.instance_id, permit);
    drop(client);
    match authorization {
        Ok(authorization) if authorization.may_exec => {}
        Ok(_) => {
            report(
                fd,
                HelperPhase::Refused {
                    code: ErrorCode::InvalidTransition,
                    message: "this launch was already authorized; a replay never execs".into(),
                },
            );
            return NOT_AUTHORIZED_STATUS;
        }
        Err(error) => {
            // A refusal, or a reply that was lost: either way no exec.
            report(
                fd,
                HelperPhase::Refused {
                    code: error.code,
                    message: error.message,
                },
            );
            return NOT_AUTHORIZED_STATUS;
        }
    }
    report(fd, HelperPhase::Ready {});
    // Command::exec restores the default SIGPIPE disposition and signal mask.
    let error = Command::new(&arguments.program)
        .args(&arguments.args)
        .exec();
    let errno = error.raw_os_error().unwrap_or(0);
    report(fd, HelperPhase::ExecFailed { errno });
    if errno == libc::ENOENT {
        NOT_FOUND_STATUS
    } else {
        EXEC_FAILED_STATUS
    }
}

fn main() {
    let Some(arguments) = parse() else {
        // Without valid descriptors there is no private channel to report on.
        eprintln!("devguard-launch: invalid invocation; the executable was not started");
        std::process::exit(NOT_AUTHORIZED_STATUS);
    };
    let status = if arguments.clamped {
        authorized_exec(arguments)
    } else {
        clamp(&arguments)
    };
    std::process::exit(status);
}
