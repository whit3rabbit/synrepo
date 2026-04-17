//! Bootstrap: first-run UX and health checks.
//!
//! Spec: `openspec/specs/bootstrap/spec.md`

mod init;
mod mode_inspect;
mod report;
pub mod runtime_probe;

pub use init::bootstrap;
pub use report::{BootstrapAction, BootstrapHealth, BootstrapReport};
pub use runtime_probe::{
    probe, AgentIntegration, AgentTargetKind, Missing, ProbeReport, RoutingDecision,
    RuntimeClassification,
};
