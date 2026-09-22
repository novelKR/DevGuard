//! Bounded foreground inspection example; no workload registration or execution.
use devguard_client::{credential::take_inherited, protocol::CallerCredential, Client};
use devguard_contract::{Compatibility, Error, ErrorCode, Result, PROTOCOL_VERSION};
use std::collections::BTreeSet;
use std::path::Path;

fn invalid() -> Error {
    Error::new(
        ErrorCode::InvalidRequest,
        "usage: inspect SOCKET UID CONSUMER GENERATION CREDENTIAL_FD",
    )
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(invalid());
    }
    let uid = args[1].parse().map_err(|_| invalid())?;
    let fd = args[4].parse().map_err(|_| invalid())?;
    // SAFETY: this dedicated executable takes ownership of the descriptor passed
    // by its caller, before creating any clients or descriptors of its own.
    let secret = unsafe { take_inherited(fd) }?;
    let mut client = Client::connect(
        Path::new(&args[0]),
        uid,
        Compatibility {
            minimum_protocol: PROTOCOL_VERSION,
            maximum_protocol: PROTOCOL_VERSION,
            required: BTreeSet::new(),
        },
    )?;
    client.authenticate(CallerCredential::Consumer {
        consumer_id: args[2].clone(),
        generation: args[3].clone(),
        secret,
    })?;
    let status = client.status()?;
    println!("{}", serde_json::to_string(&serde_json::json!({"peer":client.hello.authority,"caller":client.hello.caller,"status":status})).map_err(|_| invalid())?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
