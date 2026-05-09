//! CLI argument types for synrepo.
//!
//! Declarative clap derives; runtime dispatch lives in `cli.rs`.

mod agent_hook;
mod command;
mod convert;
mod embeddings;
mod graph;
mod subcommands;

use clap::Parser;
use std::path::PathBuf;

pub(crate) use agent_hook::*;
pub(crate) use command::{Command, SearchModeArg};
pub(crate) use convert::*;
pub(crate) use embeddings::*;
pub(crate) use graph::*;
pub(crate) use subcommands::*;

#[derive(Parser)]
#[command(name = "synrepo")]
#[command(
    about = "A local repository map for AI coding agents",
    long_about = None
)]
#[command(version)]
pub(crate) struct Cli {
    /// Override the repo root. Defaults to the current directory.
    #[arg(long, global = true)]
    pub(crate) repo: Option<PathBuf>,

    /// Disable colored / styled TUI rendering. Applies to the dashboard and
    /// first-run wizards. Honored by the bare-`synrepo` entrypoint as well.
    #[arg(long, global = true)]
    pub(crate) no_color: bool,

    /// Subcommand. Omit to run the smart entrypoint (runtime probe + router).
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[cfg(test)]
mod tests;
