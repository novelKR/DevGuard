use devguard_contract::{Error, ErrorCode, Result};
use devguard_core::AuthorityStorage;
use devguard_daemon::{
    candidate::{Candidate, CandidateSpec, Closure},
    config::{self, HostConfig},
    install::{self, InstallOptions, Launchctl},
    paths::AuthorityPaths,
    server::Server,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

/// Run `serve` with a stop flag that SIGINT and SIGTERM set.
fn until_signalled<T>(serve: impl FnOnce(Arc<AtomicBool>) -> Result<T>) -> Result<T> {
    // SAFETY: handlers only store to a lock-free atomic; no allocation or I/O.
    unsafe {
        libc::signal(libc::SIGINT, stop_signal as *const () as libc::sighandler_t);
        libc::signal(
            libc::SIGTERM,
            stop_signal as *const () as libc::sighandler_t,
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    let watched = stop.clone();
    let watcher = std::thread::spawn(move || {
        while !watched.load(Ordering::Relaxed) {
            if STOP.load(Ordering::Relaxed) {
                watched.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let result = serve(stop.clone());
    stop.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    result
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["help"] {
        println!("devguardd paths | init | check | serve | status | install --package DIR | version --json\n          candidate --id ID --lease CONSUMER/GENERATION/ATTEMPT --capacity MILLICPU,BYTES,TASKS --token-fd N\nNormal authority paths come from the OS account. No path or test-budget override is accepted.\nserve observes native boot, process and host pressure evidence and, with it, opens registration, fenced launch through devguard-launch and reconciliation; `devguard` runs commands through it. install copies a package into a protected release and starts it as the current user's LaunchAgent; it must run from that package and refuses while an authority or the agent exists. status reports the installed service. init and check never start workloads.\ncandidate serves an isolated candidate authority whose capacity is a parent lease of the account's authority; it reads the lease token from the private descriptor N, launches nothing, and closes when the lease ends. `devguard test-candidate` starts it as a lease child.");
        return Ok(());
    }
    let command = args.first().map(String::as_str).unwrap_or_default();
    let valid = match command {
        "paths" | "init" | "check" | "serve" | "status" => args.len() == 1,
        "install" => args.len() == 3 && args[1] == "--package",
        "candidate" => args.len() == 9,
        "version" => args.len() == 2 && args[1] == "--json",
        _ => false,
    };
    if !valid {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "expected paths, init, check, serve, status, install --package DIR, candidate or version --json; no alternate authority arguments are supported",
        ));
    }
    if command == "version" {
        println!(
            "{}",
            serde_json::to_string(&install::compiled())
                .map_err(|_| Error::new(ErrorCode::InvalidRequest, "version encoding failed"))?
        );
        return Ok(());
    }
    let paths = AuthorityPaths::current_user()?;
    match command {
        "install" => {
            let report = install::install(
                &paths,
                std::path::Path::new(&args[2]),
                &Launchctl::new(paths.uid()),
                &InstallOptions::canonical(&paths),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|_| Error::new(ErrorCode::InvalidRequest, "report encoding failed"))?
            );
        }
        "status" => {
            let report = install::status(
                &paths,
                &Launchctl::new(paths.uid()),
                &InstallOptions::canonical(&paths),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|_| Error::new(ErrorCode::InvalidRequest, "report encoding failed"))?
            );
            if !report.healthy {
                std::process::exit(1);
            }
        }
        "paths" => println!(
            "{}",
            serde_json::to_string(&paths)
                .map_err(|_| Error::new(ErrorCode::InvalidRequest, "path encoding failed"))?
        ),
        "init" => {
            config::initialize(&paths)?;
            println!("Explicit bootstrap complete; runtime admission remains unavailable.");
        }
        "check" => {
            paths.validate_existing()?;
            let config = HostConfig::load(&paths)?;
            let _storage = AuthorityStorage::open(&paths.journal())?;
            println!(
                "{}",
                serde_json::json!({"configuration":config.fingerprint()?,"journal_valid":true,"runtime_ready":false,"reason":"check validates storage only; serve opens registration, launch and reconciliation with native host evidence"})
            );
        }
        "serve" => {
            let server = Server::open(&paths)?;
            until_signalled(|stop| server.run(stop))?;
        }
        "candidate" => {
            let (spec, fd) = CandidateSpec::parse(&args[1..])?;
            // SAFETY: the descriptor was passed to this process for the
            // lease token only; it is consumed and closed here.
            let token = unsafe { devguard_client::credential::take_inherited(fd) }?;
            let candidate = Candidate::open(&paths, spec, token, None)?;
            // An unconfirmed lease fails closed; an ended lease or a stop
            // signal is an ordinary end.
            if let Closure::Unconfirmed { error } = until_signalled(|stop| candidate.serve(stop))? {
                return Err(error);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
