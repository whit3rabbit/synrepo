//! Explain-step types for the setup wizard.
//!
//! Kept out of `state.rs` so the state machine file stays under the 400-line
//! limit and so the explain choice + local-endpoint presets have a single
//! place to live. The plan shape (`ExplainChoice`) is also what the bin-side
//! dispatcher pattern-matches on when patching repo-local `.synrepo/config.toml`
//! and user-scoped `~/.synrepo/config.toml`.

pub mod input;
pub mod providers;
pub mod support;

pub use input::TextInputField;
pub use providers::{CloudProvider, LocalPreset, LOCAL_PRESETS};
pub use support::{
    CloudCredentialSource, ExplainChoice, ExplainRow, ExplainWizardSupport, EXPLAIN_ROWS,
};
