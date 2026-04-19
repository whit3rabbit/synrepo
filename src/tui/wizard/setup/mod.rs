//! Setup wizard: guided first-run flow for uninitialized repos.
//!
//! Four steps — splash, graph mode, agent target, confirm — and then the
//! wizard returns a [`SetupPlan`] the bin-side dispatcher executes.
//! Cancellation at any point before Confirm guarantees zero writes. The state
//! machine is deliberately trivial (no `async`, no shared state) so the unit
//! tests can exercise it by driving `handle_key` with crafted key events and
//! asserting on the resulting plan.

pub mod render;
pub mod state;
pub mod synthesis;
mod tests;

pub use render::{run_setup_wizard_loop, run_synthesis_only_wizard_loop};
pub use state::{SetupPlan, SetupWizardOutcome, SetupWizardState, WIZARD_TARGETS};
pub use synthesis::{
    CloudProvider, LocalPreset, SynthesisChoice, SynthesisRow, TextInputField, LOCAL_PRESETS,
    SYNTHESIS_ROWS,
};
