//! Agent-integration sub-wizard. Launched from the dashboard quick action so
//! operators can write a shim, register the MCP server, or both — without
//! leaving the TUI. Destructive actions (overwriting an existing shim whose
//! content differs from canonical) are gated behind an explicit toggle.
//!
//! Like the other wizards in this module, this one produces an
//! [`IntegrationPlan`] that the bin-side dispatcher executes after the TUI
//! alt-screen tears down. The library never calls the bin-side `step_*`
//! helpers directly.

pub mod state;
pub mod render;
mod tests;

pub use state::{IntegrationPlan, IntegrationWizardOutcome, IntegrationWizardState, ActionRow, IntegrationStep};
pub use render::run_integration_wizard_loop;