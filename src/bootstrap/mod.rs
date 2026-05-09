//! Bootstrap: first-run UX and health checks.
//!
//! Spec: `openspec/specs/bootstrap/spec.md`

mod init;
mod mode_inspect;
mod report;
pub mod runtime_probe;

pub use init::{
    bootstrap, bootstrap_with_force, bootstrap_with_force_and_config, ensure_root_gitignore_entry,
    remove_from_root_gitignore, root_gitignore_contains_synrepo,
};
pub use report::{BootstrapAction, BootstrapHealth, BootstrapReport};
pub use runtime_probe::{
    probe, AgentIntegration, AgentTargetKind, Missing, ProbeReport, RoutingDecision,
    RuntimeClassification,
};
