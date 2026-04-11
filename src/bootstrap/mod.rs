//! Bootstrap: first-run UX and health checks.
//!
//! Spec: `openspec/specs/bootstrap/spec.md`

mod init;
mod mode_inspect;
mod report;

pub use init::bootstrap;
pub use report::{BootstrapAction, BootstrapHealth, BootstrapReport};
