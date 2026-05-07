//! Repo-local MCP install picker. Launched from the dashboard MCP tab so
//! operators can install project-scoped agent guidance and register
//! `synrepo mcp --repo .` for one local agent target without running the
//! generic integration wizard.

pub mod render;
pub mod state;
#[cfg(test)]
mod tests;

pub use render::run_mcp_install_wizard_loop;
pub use state::{McpInstallPlan, McpInstallStep, McpInstallWizardOutcome, McpInstallWizardState};
