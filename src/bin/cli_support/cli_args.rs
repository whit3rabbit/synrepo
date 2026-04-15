//! CLI argument types for synrepo.
//!
//! Pure declarative clap derives with zero runtime logic.
//! The dispatcher lives in `cli.rs`.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use synrepo::config::Mode;
use synrepo::pipeline::export::ExportFormat;

use super::agent_shims::AgentTool;

#[derive(Parser)]
#[command(name = "synrepo")]
#[command(about = "A context compiler for AI coding agents", long_about = None)]
#[command(version)]
pub(crate) struct Cli {
    /// Override the repo root. Defaults to the current directory.
    #[arg(long, global = true)]
    pub(crate) repo: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
    Status {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
        /// Include recent operational activity (reconcile, repair, overlay events).
        #[arg(long)]
        recent: bool,
    },

    /// Generate a thin integration shim for the specified agent CLI.
    ///
    /// Writes a named fragment file and prints the one-line include instruction.
    /// Never modifies existing configuration files. Use `--force` to overwrite,
    /// or `--regen` to compare and overwrite only if the content has changed.
    AgentSetup {
        /// Target agent CLI.
        tool: AgentTool,
        /// Overwrite an existing shim file if one already exists.
        #[arg(long)]
        force: bool,
        /// Compare existing file against the current template; overwrite if different.
        #[arg(long)]
        regen: bool,
    },

    /// Run a structural compile pass against the current repository state.
    ///
    /// Requires `.synrepo/` to be initialized (`synrepo init`). Re-reads all
    /// source files, refreshes the graph store, and rebuilds the substrate
    /// index without recreating the full runtime layout.
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
        /// Generate new cross-link candidates for the whole repository.
        #[arg(long)]
        generate_cross_links: bool,
        /// Re-run generation for stale candidates.
        #[arg(long)]
        regenerate_cross_links: bool,
    },

    /// Lexical search via the syntext index.
    Search {
        /// The query string.
        query: String,
        /// Match case-insensitively.
        #[arg(short = 'i', long = "ignore-case")]
        ignore_case: bool,
        /// Restrict to one file extension such as `rs` or `py`.
        #[arg(short = 't', long = "type")]
        file_type: Option<String>,
        /// Exclude one file extension such as `js`.
        #[arg(short = 'T', long = "exclude-type")]
        exclude_type: Option<String>,
        /// Restrict to paths matching a glob such as `src/` or `**/*.rs`.
        #[arg(short = 'g', long = "glob")]
        path_filter: Option<String>,
        /// Stop after this many matches.
        #[arg(short = 'm', long = "max-results")]
        max_results: Option<usize>,
    },

    /// Graph-level queries and inspection.
    #[command(subcommand)]
    Graph(GraphCommand),

    /// Dump a node's metadata by ID.
    Node {
        /// The node ID in display format (for example `file_0000000000000042`).
        id: String,
    },
    /// Watch the current repository and keep `.synrepo/` fresh.
    Watch {
        /// Start the watcher as a detached daemon.
        #[arg(long)]
        daemon: bool,
        /// Optional watch control subcommand.
        #[command(subcommand)]
        command: Option<WatchCommand>,
    },
    /// Proposed overlay cross-links interactions.
    #[command(subcommand)]
    Links(LinksCommand),

    /// Evaluate and apply storage compatibility actions for `.synrepo/`.
    ///
    /// Dry-run by default: prints a plan table (store, action, reason) and exits.
    /// Pass `--apply` to execute non-blocking actions in dependency order and run
    /// a reconcile pass if any stores were rebuilt.
    Upgrade {
        /// Execute the compatibility actions instead of printing a dry-run plan.
        #[arg(long)]
        apply: bool,
    },

    /// Generate export files (markdown or JSON snapshots) in the configured export directory.
    ///
    /// Produces `synrepo-context/` (or the configured directory) with rendered card output.
    /// The directory is added to `.gitignore` unless `--commit` is passed.
    Export {
        /// Output format.
        #[arg(long, default_value = "markdown")]
        format: ExportFormatArg,
        /// Use Deep budget (more detail; slower).
        #[arg(long)]
        deep: bool,
        /// Track the export directory in source control (suppress .gitignore insertion).
        #[arg(long)]
        commit: bool,
        /// Override the export directory from config.
        #[arg(long)]
        out: Option<String>,
    },

    /// Search and retrieve proposed cross-links and their provenance.
    Findings {
        /// Target node endpoint ID
        #[arg(long)]
        node: Option<String>,
        /// Filter by edge kind (references, governs, derived_from, mentions)
        #[arg(long)]
        kind: Option<String>,
        /// Filter by freshness state (fresh, stale, source_deleted)
        #[arg(long)]
        freshness: Option<String>,
        /// Maximum number of findings to return
        #[arg(long)]
        limit: Option<usize>,
        /// Emit JSON instead of human-readable output
        #[arg(long)]
        json: bool,
    },

    #[command(name = "watch-internal", hide = true)]
    WatchInternal,

    /// Start the MCP server over stdio.
    ///
    /// Exposes 16 read-only tools for coding agents to query the repository's
    /// structural graph, cards, search index, overlay data, and provenance.
    /// Communicates over stdio using the MCP protocol.
    Mcp,
}

#[derive(Subcommand)]
pub(crate) enum WatchCommand {
    /// Show watch-service status for the current repo.
    Status,
    /// Stop the active watch service for the current repo.
    Stop,
}

#[derive(Subcommand)]
pub(crate) enum LinksCommand {
    /// List all generated proposed cross-links.
    List {
        /// Filter by confidence tier
        #[arg(long)]
        tier: Option<String>,
        /// Emit JSON instead of human-readable output
        #[arg(long)]
        json: bool,
    },
    /// Review review-queue candidates awaiting manual acceptance.
    Review {
        /// Maximum number of candidates to return
        #[arg(long)]
        limit: Option<usize>,
        /// Emit JSON instead of human-readable output
        #[arg(long)]
        json: bool,
    },
    /// Accept a proposed cross-link and mutate graph edge (curated mode).
    Accept {
        /// The candidate UUID string
        candidate_id: String,
        /// Optional reviewer identity
        #[arg(long)]
        reviewer: Option<String>,
    },
    /// Reject a proposed cross-link.
    Reject {
        /// The candidate UUID string
        candidate_id: String,
        /// Optional reviewer identity
        #[arg(long)]
        reviewer: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum GraphCommand {
    /// Run a narrow traversal query against the graph store.
    Query {
        /// Query syntax: `<direction> <node_id> [edge_kind]`.
        q: String,
    },

    /// Print graph statistics (node count by type, edge count by kind).
    Stats,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ModeArg {
    Auto,
    Curated,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ExportFormatArg {
    Markdown,
    Json,
}

impl From<ExportFormatArg> for ExportFormat {
    fn from(arg: ExportFormatArg) -> Self {
        match arg {
            ExportFormatArg::Markdown => ExportFormat::Markdown,
            ExportFormatArg::Json => ExportFormat::Json,
        }
    }
}

impl From<ModeArg> for Mode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::Auto => Mode::Auto,
            ModeArg::Curated => Mode::Curated,
        }
    }
}
