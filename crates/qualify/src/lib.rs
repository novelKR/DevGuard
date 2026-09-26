//! DevGuard's native qualification harness (DG1-C12): the standalone control
//! probes and the bounded workloads of the macOS SLO protocol. It measures a
//! service and is never part of a release package; `scripts/measure.py`
//! drives it together with the foreground fixture.

pub mod control;
pub mod work;
