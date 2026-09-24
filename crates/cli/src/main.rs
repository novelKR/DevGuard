//! `devguard`: the command-line owner for managed execution.

use devguard_contract::{Error, ErrorCode};
use devguard_daemon::paths::AuthorityPaths;

fn main() {
    let exit = devguard_cli::run(std::env::args_os().skip(1), || {
        let paths = AuthorityPaths::current_user()?;
        let helper = devguard_cli::exec::sibling_helper().map_err(|_| {
            Error::new(
                ErrorCode::ResourceControlUnavailable,
                "cannot locate devguard-launch beside devguard",
            )
        })?;
        Ok((paths, helper))
    });
    devguard_cli::terminate(exit)
}
