//! synrepo CLI entry point.
//!
//! Phase 0/1 subcommands:
//! - `synrepo init [--mode auto|curated]`, create `.synrepo/` in the current repo
//! - `synrepo status`, print operational health: mode, graph counts, last reconcile, lock state
//! - `synrepo agent-setup <tool>`, generate a thin integration shim for claude/cursor/copilot/generic
//! - `synrepo reconcile`, run a structural compile pass without full re-bootstrap
//! - `synrepo check`, read-only drift report across all repair surfaces
//! - `synrepo sync`, repair auto-fixable drift surfaces and log the outcome
//! - `synrepo search <query>`, lexical search against the persisted index
//! - `synrepo graph query "<direction> <node_id> [edge_kind]"`, narrow graph traversal query
//! - `synrepo node <id>`, dump a node's metadata
//!
//! All non-trivial logic lives in the library crate or local support modules.

mod cli_support;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use cli_support::agent_shims::AgentTool;
use cli_support::commands::{
    agent_setup, check, graph_query, graph_stats, init, node, reconcile, search, status, sync,
};
use synrepo::config::Mode;

#[derive(Parser)]
#[command(name = "synrepo")]
#[command(about = "A context compiler for AI coding agents", long_about = None)]
#[command(version)]
struct Cli {
    /// Override the repo root. Defaults to the current directory.
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a `.synrepo/` directory in the current repo.
    Init {
        /// Operational mode.
        #[arg(long, value_enum)]
        mode: Option<ModeArg>,
    },

    /// Print operational health: mode, graph node counts, last reconcile outcome, and writer lock state.
    ///
    /// Reads only; never acquires the writer lock or mutates any store.
    /// Safe to run at any time, including while a reconcile is in progress.
    Status,

    /// Generate a thin integration shim for the specified agent CLI.
    ///
    /// Writes a named fragment file and prints the one-line include instruction.
    /// Never modifies existing configuration files. Use `--force` to overwrite.
    AgentSetup {
        /// Target agent CLI.
        tool: AgentTool,
        /// Overwrite an existing shim file if one already exists.
        #[arg(long)]
        force: bool,
    },

    /// Run a structural compile pass against the current repository state.
    ///
    /// Requires `.synrepo/` to be initialized (`synrepo init`). Re-reads all
    /// source files and refreshes the graph store without recreating the full
    /// runtime layout or re-indexing the substrate.
    Reconcile,

    /// Report drift across all repair surfaces. Read-only; never mutates state.
    ///
    /// Inspects storage compatibility, reconcile health, declared links, and
    /// unsupported surfaces. Exits non-zero if any actionable or blocked
    /// findings are present.
    Check {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Repair auto-fixable drift surfaces and record the outcome.
    ///
    /// Runs storage maintenance and a structural reconcile for actionable
    /// findings. Report-only and unsupported findings are surfaced but left
    /// untouched. Appends an entry to `.synrepo/state/repair-log.jsonl`.
    Sync {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Lexical search via the syntext index.
    Search {
        /// The query string.
        query: String,
    },

    /// Graph-level queries and inspection.
    #[command(subcommand)]
    Graph(GraphCommand),

    /// Dump a node's metadata by ID.
    Node {
        /// The node ID in display format (for example `file_0000000000000042`).
        id: String,
    },
}

#[derive(Subcommand)]
enum GraphCommand {
    /// Run a narrow traversal query against the graph store.
    Query {
        /// Query syntax: `<direction> <node_id> [edge_kind]`.
        q: String,
    },

    /// Print graph statistics (node count by type, edge count by kind).
    Stats,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ModeArg {
    Auto,
    Curated,
}

impl From<ModeArg> for Mode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::Auto => Mode::Auto,
            ModeArg::Curated => Mode::Curated,
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let repo_root = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine working directory: {e}"))?,
    };

    match cli.command {
        Command::Init { mode } => init(&repo_root, mode.map(Into::into)),
        Command::Status => status(&repo_root),
        Command::AgentSetup { tool, force } => agent_setup(&repo_root, tool, force),
        Command::Reconcile => reconcile(&repo_root),
        Command::Check { json } => check(&repo_root, json),
        Command::Sync { json } => sync(&repo_root, json),
        Command::Search { query } => search(&repo_root, &query),
        Command::Graph(GraphCommand::Query { q }) => graph_query(&repo_root, &q),
        Command::Graph(GraphCommand::Stats) => graph_stats(&repo_root),
        Command::Node { id } => node(&repo_root, &id),
    }
}
