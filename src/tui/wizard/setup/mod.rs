//! Setup wizard: guided first-run flow for uninitialized repos.
//!
//! Multi-step guided flow: splash, graph mode, agent target, explain choice,
//! optional cloud-key or local-endpoint details, and final confirm. The wizard
//! returns a [`SetupPlan`] the bin-side dispatcher executes.
//! Cancellation at any point before Confirm guarantees zero writes. The state
//! machine is deliberately trivial (no `async`, no shared state) so the unit
//! tests can exercise it by driving `handle_key` with crafted key events and
//! asserting on the resulting plan.

pub mod explain;
pub mod render;
pub mod state;
pub(super) mod state_types;
mod tests;

pub use explain::{
    CloudCredentialSource, CloudProvider, ExplainChoice, ExplainRow, ExplainWizardSupport,
    LocalPreset, TextInputField, EXPLAIN_ROWS, LOCAL_PRESETS,
};
pub use render::{run_explain_only_wizard_loop, run_setup_wizard_loop};
pub use state_types::{SetupPlan, SetupStep, SetupWizardOutcome, SetupWizardState, WIZARD_TARGETS};
