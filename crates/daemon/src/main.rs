use devguard_contract::{Error, ErrorCode, Result};
use devguard_core::AuthorityStorage;
use devguard_daemon::{
    config::{self, HostConfig},
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

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["help"] {
        println!("devguardd paths | init | check | serve\nNormal authority paths come from the OS account. No path or test-budget override is accepted.\nserve observes native boot, process and host pressure evidence and, with it, opens registration, fenced launch through devguard-launch and reconciliation. There is no execution CLI yet; init and check never start workloads.");
        return Ok(());
    }
    if args.len() != 1 || !matches!(args[0].as_str(), "paths" | "init" | "check" | "serve") {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "expected paths, init, check or serve; no alternate authority arguments are supported",
        ));
    }
    let paths = AuthorityPaths::current_user()?;
    match args[0].as_str() {
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
            let result = server.run(stop.clone());
            stop.store(true, Ordering::Relaxed);
            let _ = watcher.join();
            result?;
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
