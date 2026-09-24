//! Service ownership and configuration. No invented host evidence or hidden fallback.
pub mod candidate;
pub mod config;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixture;
pub mod install;
pub mod paths;
pub mod server;
