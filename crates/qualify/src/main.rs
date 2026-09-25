//! `devguard-qualify`: the control probes and bounded workloads of the macOS
//! SLO protocol. The probe uses the operating account's canonical authority,
//! like `devguard`; there is no path or authority override.

use devguard_daemon::paths::AuthorityPaths;
use devguard_qualify::{control, work};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const USAGE: &str = "\
devguard-qualify control --helper PATH --duration-s N --out FILE
              [--status-interval-ms N] [--terminate-period-s N] [--admission-wait-s N]
devguard-qualify work cpu --threads N --seconds N
devguard-qualify work memory --mib N --seconds N
devguard-qualify work io --dir DIR --mib N [--seconds N]
devguard-qualify work output --mib N [--seconds N]
devguard-qualify work slow-reader

control registers as a dev-cli owner of the canonical authority, keeps one target running for
status calls and starts a fresh target for each termination, all through the given
devguard-launch, and writes one JSON sample per line to the new file FILE. SIGINT, SIGTERM or SIGHUP
ends sampling early, as does the loss of the probe's parent; the probe still settles its targets,
which live at most ten minutes beyond the sampling. work runs one bounded workload and prints
what it did.";

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(summary) if summary.is_empty() => {}
        Ok(summary) => println!("{summary}"),
        Err(message) => {
            eprintln!("devguard-qualify: {message}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

/// Flags given as `--name value`, each at most once and each one of `known`.
fn parse<'a>(args: &'a [String], known: &[&str]) -> Result<BTreeMap<&'a str, &'a str>, String> {
    let mut flags = BTreeMap::new();
    let mut rest = args.iter();
    while let Some(name) = rest.next() {
        if !known.contains(&name.as_str()) {
            return Err(format!("unexpected argument {name}"));
        }
        let value = rest.next().ok_or_else(|| format!("{name} needs a value"))?;
        if flags.insert(name.as_str(), value.as_str()).is_some() {
            return Err(format!("{name} is given twice"));
        }
    }
    Ok(flags)
}

fn required<'a>(flags: &BTreeMap<&str, &'a str>, name: &str) -> Result<&'a str, String> {
    flags
        .get(name)
        .copied()
        .ok_or_else(|| format!("{name} is required"))
}

fn number(flags: &BTreeMap<&str, &str>, name: &str, default: Option<u64>) -> Result<u64, String> {
    match flags.get(name) {
        Some(value) => value
            .parse()
            .map_err(|_| format!("{name} must be a whole number")),
        None => default.ok_or_else(|| format!("{name} is required")),
    }
}

fn run(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("control") => probe(&args[1..]),
        Some("work") => workload(&args[1..]),
        Some("--help") => Ok(USAGE.into()),
        _ => Err("expected control or work".into()),
    }
}

fn probe(args: &[String]) -> Result<String, String> {
    let flags = parse(
        args,
        &[
            "--helper",
            "--duration-s",
            "--out",
            "--status-interval-ms",
            "--terminate-period-s",
            "--admission-wait-s",
        ],
    )?;
    let helper = PathBuf::from(required(&flags, "--helper")?);
    if !helper.is_absolute() {
        return Err("--helper must be an absolute path".into());
    }
    let mut options =
        control::Options::protocol(Duration::from_secs(number(&flags, "--duration-s", None)?));
    options.status_interval = Duration::from_millis(number(
        &flags,
        "--status-interval-ms",
        Some(options.status_interval.as_millis() as u64),
    )?);
    options.terminate_period = Duration::from_secs(number(
        &flags,
        "--terminate-period-s",
        Some(options.terminate_period.as_secs()),
    )?);
    options.admission_wait = Duration::from_secs(number(
        &flags,
        "--admission-wait-s",
        Some(options.admission_wait.as_secs()),
    )?);
    if options.status_interval.is_zero() || options.terminate_period.is_zero() {
        return Err("intervals must be positive".into());
    }
    let out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(required(&flags, "--out")?)
        .map_err(|error| format!("cannot create the sample file: {error}"))?;
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    // A closed terminal (SIGHUP) stops sampling like SIGINT and SIGTERM, so
    // the probe still settles its targets.
    unsafe {
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(signal, stop as *const () as libc::sighandler_t);
        }
    }
    let paths = AuthorityPaths::current_user().map_err(|error| error.message)?;
    let mut out = BufWriter::new(out);
    control::run(&paths, &helper, &options, &mut out, &STOP)
        .map(|summary| summary.to_string())
        .map_err(|error| error.message)
}

fn workload(args: &[String]) -> Result<String, String> {
    let io_error = |error: std::io::Error| error.to_string();
    let rest = args.get(1..).unwrap_or_default();
    let summary = match args.first().map(String::as_str) {
        Some("cpu") => {
            let flags = parse(rest, &["--threads", "--seconds"])?;
            let threads = number(&flags, "--threads", None)? as usize;
            let seconds = Duration::from_secs(number(&flags, "--seconds", None)?);
            json!({"work": "cpu", "threads": threads, "rounds": work::cpu(threads, seconds)})
        }
        Some("memory") => {
            let flags = parse(rest, &["--mib", "--seconds"])?;
            let mib = number(&flags, "--mib", None)? as usize;
            let seconds = Duration::from_secs(number(&flags, "--seconds", None)?);
            json!({"work": "memory", "mib": mib, "passes": work::memory(mib, seconds)})
        }
        Some("io") => {
            let flags = parse(rest, &["--dir", "--mib", "--seconds"])?;
            let directory = PathBuf::from(required(&flags, "--dir")?);
            let mib = number(&flags, "--mib", None)? as usize;
            let seconds = Duration::from_secs(number(&flags, "--seconds", Some(0))?);
            let verified = work::io_paced(&directory, mib, seconds).map_err(io_error)?;
            json!({"work": "io", "bytes_verified": verified})
        }
        Some("output") => {
            let flags = parse(rest, &["--mib", "--seconds"])?;
            let mib = number(&flags, "--mib", None)? as usize;
            let seconds = Duration::from_secs(number(&flags, "--seconds", Some(0))?);
            let written =
                work::output(mib, seconds, &mut std::io::stdout().lock()).map_err(io_error)?;
            // Standard output is the payload, so the summary goes to standard error.
            eprintln!("{}", json!({"work": "output", "bytes": written}));
            return Ok(String::new());
        }
        Some("slow-reader") => {
            parse(rest, &[])?;
            let lines = work::slow_reader(std::io::stdin().lock()).map_err(io_error)?;
            json!({"work": "slow-reader", "lines": lines})
        }
        _ => return Err("expected cpu, memory, io, output or slow-reader".into()),
    };
    Ok(summary.to_string())
}
