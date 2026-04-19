//! Uninstall wizard: guided artifact removal for `synrepo remove`.
//!
//! Given the set of detected install artifacts (shims, MCP entries, root
//! .gitignore lines, the `.synrepo/` directory), the wizard lets the operator
//! toggle each one and confirm before any mutation. Installed rows start
//! checked so the default is "remove everything"; `DeleteSynrepoDir` is the
//! single destructive row and starts unchecked.
//!
//! The wizard only returns an [`UninstallPlan`]; the bin-side dispatcher
//! translates the plan back into its own `RemoveAction` list and applies it
//! after the TUI alt-screen has been torn down, matching the pattern used by
//! the repair and integration wizards.

pub mod render;
pub mod state;
mod tests;

pub use render::run_uninstall_wizard_loop;
pub use state::{
    ActionRow as UninstallActionRow, UninstallActionKind, UninstallPlan, UninstallStep,
    UninstallWizardOutcome, UninstallWizardState,
};
