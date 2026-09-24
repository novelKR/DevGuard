#![cfg(target_os = "macos")]
//! DG1-C07: job control on an interactive terminal. The CLI hands the
//! terminal to the workload's group, so terminal keys reach the workload
//! directly, and mirrors a stopped workload so the shell regains control.

mod support;
use serde_json::{json, Value};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::time::Duration;
use support::*;

#[test]
#[ignore = "the CLI process of the terminal tests"]
fn cli_child() {
    support::cli_child();
}

#[test]
#[ignore = "the workload of the terminal tests"]
fn payload() {
    support::payload();
}

/// A minimal job-control shell: it runs the CLI as a job in its own group on
/// this session's terminal, reports a stop, takes the terminal back as a
/// shell does, then resumes the job in the foreground and waits for it.
#[test]
#[ignore = "a job-control shell for the terminal tests"]
fn shell_child() {
    let Some(config) = std::env::var_os("DEVGUARD_TEST_SHELL") else {
        return;
    };
    let config: Value = serde_json::from_str(&config.to_string_lossy()).unwrap();
    let args: Vec<String> = serde_json::from_value(config["args"].clone()).unwrap();
    let mut command = Command::new(exe());
    command
        .args(probe_args("cli_child"))
        .env(
            CLI_CHILD,
            json!({"base": config["base"], "helper": helper(), "args": args}).to_string(),
        )
        .env(PROBE, config["probe"].as_str().unwrap())
        .process_group(0);
    // As a shell does, the job also takes the terminal itself before exec,
    // so the CLI never observes itself in the background.
    // SAFETY: only async-signal-safe calls on this child's own group.
    unsafe {
        command.pre_exec(|| {
            let mut block: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut block);
            libc::sigaddset(&mut block, libc::SIGTTOU);
            libc::pthread_sigmask(libc::SIG_BLOCK, &block, std::ptr::null_mut());
            let result = libc::tcsetpgrp(0, libc::getpgrp());
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &block, std::ptr::null_mut());
            if result != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let job = command.spawn().unwrap();
    let pid = job.id() as libc::pid_t;
    let own = unsafe { libc::getpgrp() };
    let set_foreground = |group: libc::pid_t| {
        // SAFETY: blocking SIGTTOU around tcsetpgrp on descriptor 0, as a
        // shell does when it changes the foreground job.
        unsafe {
            let mut block: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut block);
            libc::sigaddset(&mut block, libc::SIGTTOU);
            libc::pthread_sigmask(libc::SIG_BLOCK, &block, std::ptr::null_mut());
            let result = libc::tcsetpgrp(0, group);
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &block, std::ptr::null_mut());
            result
        }
    };
    assert_eq!(set_foreground(pid), 0);
    let mut events = Vec::new();
    loop {
        let mut status = 0;
        // SAFETY: waiting for the shell's own child.
        let waited = unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) };
        assert_eq!(waited, pid);
        if libc::WIFSTOPPED(status) {
            // The job stopped: the shell takes the terminal back, then
            // resumes the job in the foreground, as `fg` does.
            let holder = unsafe { libc::tcgetpgrp(0) };
            events.push(
                json!({"stopped": libc::WSTOPSIG(status), "terminal_holder": holder, "job": pid}),
            );
            assert_eq!(set_foreground(own), 0);
            std::thread::sleep(Duration::from_millis(300));
            assert_eq!(set_foreground(pid), 0);
            // SAFETY: continuing the shell's own stopped job group.
            unsafe { libc::killpg(pid, libc::SIGCONT) };
            continue;
        }
        let exit = if libc::WIFEXITED(status) {
            json!({"code": libc::WEXITSTATUS(status)})
        } else {
            json!({"signal": libc::WTERMSIG(status)})
        };
        events.push(json!({"exited": exit}));
        break;
    }
    std::fs::write(
        config["out"].as_str().unwrap(),
        json!({"events": events}).to_string(),
    )
    .unwrap();
    std::process::exit(0);
}

struct Pty {
    master: OwnedFd,
    slave: OwnedFd,
}

fn pty() -> Pty {
    let (mut master, mut slave) = (0, 0);
    // SAFETY: openpty fills two new descriptors that are owned below.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(result, 0);
    // SAFETY: the descriptors are new and owned here; keep them out of children.
    unsafe {
        libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave, libc::F_SETFD, libc::FD_CLOEXEC);
        Pty {
            master: OwnedFd::from_raw_fd(master),
            slave: OwnedFd::from_raw_fd(slave),
        }
    }
}

/// Run `command` as the leader of a new session whose controlling terminal
/// is the pty slave, on all three standard descriptors.
fn in_session(command: &mut Command, pty: &Pty) {
    command
        .stdin(Stdio::from(pty.slave.try_clone().unwrap()))
        .stdout(Stdio::from(pty.slave.try_clone().unwrap()))
        .stderr(Stdio::from(pty.slave.try_clone().unwrap()));
    // SAFETY: setsid and ioctl are async-signal-safe and act on this child.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY.into(), 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Keep reading the terminal so output never blocks the session.
fn drain(pty: &Pty) -> std::thread::JoinHandle<Vec<u8>> {
    let mut master = std::fs::File::from(pty.master.try_clone().unwrap());
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let _ = master.read_to_end(&mut output);
        output
    })
}

fn type_keys(pty: &Pty, keys: &[u8]) {
    // SAFETY: writing to the pty master this test owns.
    let written = unsafe { libc::write(pty.master.as_raw_fd(), keys.as_ptr().cast(), keys.len()) };
    assert_eq!(written, keys.len() as isize);
}

#[test]
fn the_terminal_goes_to_the_workload_so_its_keys_reach_it_directly() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (out, ready, receipt) = (
        work.join("payload.json"),
        work.join("ready"),
        work.join("receipt.json"),
    );
    let pty = pty();
    let reader = drain(&pty);
    let args = exec_payload(&[
        "--cpu",
        "50",
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    let mut command = Command::new(exe());
    command.args(probe_args("cli_child")).env(
        CLI_CHILD,
        json!({"base": fixture.base(), "helper": helper(), "args": args}).to_string(),
    );
    command.env(
        PROBE,
        json!({"out": out, "ready": ready, "sleep_ms": 30_000}).to_string(),
    );
    in_session(&mut command, &pty);
    let mut cli = command.spawn().unwrap();
    wait_for(EXIT_LIMIT, || ready.exists());
    let seen = read_json(&out);
    // The workload's group holds the terminal.
    assert_eq!(seen["tty"], true);
    assert_eq!(seen["terminal_foreground_group"], seen["pgid"]);
    // Ctrl-C from the terminal reaches the foreground workload, not the CLI.
    type_keys(&pty, b"\x03");
    let status = wait_exit(&mut cli, EXIT_LIMIT);
    assert_eq!(status.signal(), Some(libc::SIGINT), "{status:?}");
    drop(pty);
    let _ = reader.join();
    let receipt = read_json(&receipt);
    assert_eq!(receipt["launch"]["terminal_handed"], true);
    assert_eq!(receipt["exit"], json!({"signal": libc::SIGINT}));
    assert_eq!(receipt["signals"]["received"], json!([]));
    assert_eq!(receipt["command"]["tty"], true);
    record(
        "terminal-interrupt",
        json!({"payload": {"tty": seen["tty"], "pgid": seen["pgid"],
                           "terminal_foreground_group": seen["terminal_foreground_group"]},
               "cli_signal": status.signal(), "receipt_launch": receipt["launch"],
               "cli_received_signals": receipt["signals"]["received"]}),
        &fixture.secret(),
    );
}

#[test]
fn a_stopped_workload_is_mirrored_so_the_shell_regains_the_terminal() {
    let fixture = Fixture::start();
    let work = fixture.work("work");
    let (out, ready, receipt, shell_out) = (
        work.join("payload.json"),
        work.join("ready"),
        work.join("receipt.json"),
        work.join("shell.json"),
    );
    let pty = pty();
    let reader = drain(&pty);
    let args = exec_payload(&[
        "--cpu",
        "50",
        "--memory",
        "16MiB",
        "--tasks",
        "2",
        "--receipt",
        receipt.to_str().unwrap(),
    ]);
    let mut command = Command::new(exe());
    command.args(probe_args("shell_child")).env(
        "DEVGUARD_TEST_SHELL",
        json!({"base": fixture.base(), "args": args, "out": shell_out,
               "probe": json!({"out": out, "ready": ready, "sleep_ms": 1_500}).to_string()})
        .to_string(),
    );
    in_session(&mut command, &pty);
    let mut shell = command.spawn().unwrap();
    wait_for(EXIT_LIMIT, || ready.exists());
    let seen = read_json(&out);
    assert_eq!(seen["terminal_foreground_group"], seen["pgid"]);
    // Ctrl-Z stops the foreground workload; the CLI mirrors the stop.
    type_keys(&pty, b"\x1a");
    assert!(wait_exit(&mut shell, EXIT_LIMIT).success());
    drop(pty);
    let _ = reader.join();
    let shell_record = read_json(&shell_out);
    let events = shell_record["events"].as_array().unwrap();
    assert_eq!(events.len(), 2, "{shell_record}");
    assert_eq!(events[0]["stopped"], libc::SIGTSTP);
    // The CLI had taken the terminal back to its own group before stopping.
    assert_eq!(events[0]["terminal_holder"], events[0]["job"]);
    assert_eq!(events[1]["exited"], json!({"code": 0}));
    let receipt = read_json(&receipt);
    assert_eq!(receipt["launch"]["stops_mirrored"], 1);
    assert_eq!(receipt["exit"], json!({"code": 0}));
    record(
        "terminal-stop",
        json!({"shell_events": events, "receipt_launch": receipt["launch"],
               "receipt_exit": receipt["exit"]}),
        &fixture.secret(),
    );
}
