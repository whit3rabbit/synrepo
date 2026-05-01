//! Bootstrap: first-run UX and health checks.
//!
//! Spec: `openspec/specs/bootstrap/spec.md`

mod init;
mod mode_inspect;
mod report;
pub mod runtime_probe;

pub use init::{bootstrap, bootstrap_with_force, remove_from_root_gitignore};
pub use report::{BootstrapAction, BootstrapHealth, BootstrapReport};
pub use runtime_probe::{
    probe, AgentIntegration, AgentTargetKind, Missing, ProbeReport, RoutingDecision,
    RuntimeClassification,
};
