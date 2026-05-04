//! Hidden agent-hook CLI arguments.

use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub(crate) enum AgentHookCommand {
    /// Run an advisory synrepo nudge hook.
    #[command(hide = true)]
    Nudge(AgentHookNudgeArgs),
}

#[derive(Args)]
pub(crate) struct AgentHookNudgeArgs {
    /// Hook client: codex or claude.
    #[arg(long)]
    pub(crate) client: String,
    /// Hook event name, for example UserPromptSubmit or PreToolUse.
    #[arg(long)]
    pub(crate) event: String,
}
