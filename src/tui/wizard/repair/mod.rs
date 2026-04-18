//! Repair wizard: guided recovery flow for partial repos.
//!
//! Walks the operator through the [`Missing`] list produced by the runtime
//! probe, exposing toggleable repair actions. Destructive actions (in
//! particular `synrepo upgrade --apply`) default to *off* and require an
//! explicit toggle; every action is visible in a confirm step before any
//! writes happen. Cancelling at any point before the confirm step guarantees
//! `.synrepo/` stays byte-identical.
//!
//! The wizard only returns a [`RepairPlan`]; the bin-side dispatcher executes
//! the plan after the TUI alt-screen has been torn down.

pub mod render;
pub mod state;
mod tests;

pub use render::run_repair_wizard_loop;
pub use state::{
    ActionRow, RepairActionKind, RepairPlan, RepairStep, RepairWizardOutcome, RepairWizardState,
};
