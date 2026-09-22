use devguard_contract::{Error, ErrorCode, Result};
use devguard_core::AuthorityStorage;
use devguard_daemon::{
    config::{self, HostConfig},
    paths::AuthorityPaths,
};

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["help"] {
        println!("devguardd paths | init | check\nNormal authority paths come from the OS account. No path or test-budget override is accepted.\nRuntime execution is unavailable until native host evidence and launch/reconciliation are implemented.");
        return Ok(());
    }
    if args.len() != 1 || !matches!(args[0].as_str(), "paths" | "init" | "check") {
        return Err(Error::new(
            ErrorCode::InvalidRequest,
            "expected paths, init or check; no alternate authority arguments are supported",
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
                serde_json::json!({"configuration":config.fingerprint()?,"journal_valid":true,"runtime_ready":false,"reason":"native host evidence and launch are not installed"})
            );
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
