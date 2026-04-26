//! CLI argument types for synrepo.
//!
//! Pure declarative clap derives with zero runtime logic.
//! The dispatcher lives in `cli.rs`.

mod convert;
mod subcommands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use super::agent_shims::AgentTool;

pub(crate) use convert::*;
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

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Initialize a `.synrepo/` directory in the current repo.
    Init {
        /// Operational mode.
        #[arg(long, value_enum)]
        mode: Option<ModeArg>,
        /// Add .synrepo/ to the root .gitignore file.
        #[arg(long)]
        gitignore: bool,
    },

    /// Verify repo health, freshness, and readiness for agent use.
    Status {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
        /// Include recent operational activity (reconcile, repair, overlay events).
        #[arg(long)]
        recent: bool,
        /// Include the commentary freshness scan. Slow: walks every commentary
        /// row through a graph read snapshot. Default status skips it so the
        /// command stays cheap enough to run habitually.
        #[arg(long)]
        full: bool,
    },

    /// Generate the agent's skill or instructions file for the specified agent CLI.
    AgentSetup {
        /// Target agent CLI. Omit when using `--only` or `--skip`.
        #[arg(conflicts_with_all = ["only", "skip"])]
        tool: Option<AgentTool>,
        /// Comma-separated list of targets to set up. Mutually exclusive
        /// with the positional `tool` argument and with `--skip`.
        #[arg(long, value_delimiter = ',', conflicts_with = "skip")]
        only: Vec<AgentTool>,
        /// Apply to every known target except these. Comma-separated.
        /// Mutually exclusive with the positional `tool` argument and with `--only`.
        #[arg(long, value_delimiter = ',', conflicts_with = "only")]
        skip: Vec<AgentTool>,
        /// Overwrite an existing skill or instructions file if one already exists.
        #[arg(long)]
        force: bool,
        /// Compare existing file against the current template; overwrite if different.
        #[arg(long)]
        regen: bool,
    },

    /// Set up synrepo for this repo and wire an agent.
    Setup {
        /// Target client to set up. Omit to launch the interactive wizard,
        /// or pair with `--only`/`--skip` for multi-client setup.
        #[arg(conflicts_with_all = ["only", "skip"])]
        tool: Option<AgentTool>,
        /// Comma-separated list of targets to set up in one pass. Mutually
        /// exclusive with the positional `tool` argument and with `--skip`.
        #[arg(long, value_delimiter = ',', conflicts_with = "skip")]
        only: Vec<AgentTool>,
        /// Apply setup to every known target except these. Comma-separated.
        /// Mutually exclusive with the positional `tool` argument and `--only`.
        #[arg(long, value_delimiter = ',', conflicts_with = "only")]
        skip: Vec<AgentTool>,
        /// Force re-initialization and overwrite existing configs.
        #[arg(long)]
        force: bool,
        /// After the normal setup steps complete, launch the explain sub-wizard
        /// and patch repo-local `.synrepo/config.toml` plus user-scoped
        /// `~/.synrepo/config.toml` as needed.
        /// Off by default; opt-in makes the key-detected hint in `synrepo status`
        /// actionable without requiring the user to hand-edit config.
        #[arg(long)]
        explain: bool,
        /// Add .synrepo/ to the root .gitignore file.
        #[arg(long)]
        gitignore: bool,
        /// Configure the MCP server globally instead of per-project.
        #[arg(long)]
        global: bool,
    },

    /// Run a structural compile pass against the current repository state.
    Reconcile {
        /// Skip git-intensive stages (co-change and symbol revision derivation).
        #[arg(long)]
        fast: bool,
    },

    /// Install Git hooks (post-commit, post-merge, post-checkout) to trigger reconcile --fast.
    InstallHooks,

    /// Report drift across all repair surfaces. Read-only; never mutates state.
    Check {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },

    /// Repair auto-fixable drift surfaces and record the outcome.
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
        /// Rotate the explain call log and zero the per-repo totals snapshot.
        /// Does not run repair; returns after the reset.
        #[arg(long)]
        reset_explain_totals: bool,
    },

    /// Lexical search via the syntext index.
    Search {
        /// The query string.
        query: String,
        /// Match case-insensitively.
        #[arg(short = 'i', long = "ignore-case")]
        ignore_case: bool,
        /// Restrict to a specific file extension (for example `rs`, `py`).
        #[arg(short = 't', long = "type")]
        file_type: Option<String>,
        /// Exclude a specific file extension (for example `js`).
        #[arg(short = 'T', long = "exclude-type")]
        exclude_type: Option<String>,
        /// Filter results by path pattern (for example `src/`, `**/*.rs`, `tests/*_test.py`).
        #[arg(short = 'g', long = "glob")]
        path_filter: Option<String>,
        /// Stop after this many matches.
        #[arg(short = 'm', long = "max-results")]
        max_results: Option<usize>,
    },

    /// Review, search, and import editable explain docs.
    #[command(subcommand)]
    Docs(DocsCommand),

    /// Change risk assessment for a symbol or file.
    ChangeRisk {
        /// Target: file path or qualified symbol name.
        target: String,
        /// Budget tier: tiny, normal, or deep. Defaults to tiny.
        #[arg(long, short)]
        budget: Option<String>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Run an ephemeral in-memory compile for CI/PR comments.
    CiRun(CiRunArgs),

    /// Return bounded card suggestions for a task query.
    Cards {
        /// Plain-language query.
        #[arg(long)]
        query: String,
        /// Numeric token cap.
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Explain a file or symbol with a bounded card.
    Explain {
        /// Target file path or symbol name.
        target: String,
        /// Numeric token cap.
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Inspect change impact/risk before editing.
    Impact {
        /// Target file path or symbol name.
        target: String,
        /// Numeric token cap.
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Discover tests constraining a file or directory.
    Tests {
        /// Target file path or directory.
        target: String,
        /// Numeric token cap.
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Composite risk signal for a file or symbol.
    Risks {
        /// Target file path or symbol name.
        target: String,
        /// Numeric token cap.
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Aggregate synrepo statistics.
    #[command(subcommand)]
    Stats(StatsCommand),

    /// Reproducible benchmarks.
    #[command(subcommand)]
    Bench(BenchCommand),

    /// Graph-level queries and inspection.
    #[command(subcommand)]
    Graph(GraphCommand),

    /// Dump a node's metadata by ID, file path, or symbol name.
    Node {
        /// A file path (e.g. `src/lib.rs`), qualified symbol name (e.g.
        /// `my_mod::MyStruct`), or node ID (e.g. `file_0000000000000042`).
        id: String,
    },
    /// Watch the current repository and keep `.synrepo/` fresh.
    Watch {
        /// Start the watcher as a detached daemon.
        #[arg(long)]
        daemon: bool,
        /// Force plain log-line output in the foreground instead of hosting
        /// the live-mode dashboard. Non-TTY stdout (pipes, redirects, CI)
        /// already auto-falls-back to plain logs.
        #[arg(long)]
        no_ui: bool,
        /// Optional watch control subcommand.
        #[command(subcommand)]
        command: Option<WatchCommand>,
    },
    /// Proposed overlay cross-links interactions.
    #[command(subcommand)]
    Links(LinksCommand),

    /// Advisory overlay agent notes.
    #[command(subcommand)]
    Notes(NotesCommand),

    /// Evaluate and apply storage compatibility actions for `.synrepo/`.
    Upgrade {
        /// Execute the compatibility actions instead of printing a dry-run plan.
        #[arg(long)]
        apply: bool,
    },

    /// Compact overlay, state, and index stores to reclaim disk space.
    Compact {
        /// Execute the compaction actions instead of printing a dry-run plan.
        #[arg(long)]
        apply: bool,
        /// Retention policy preset (default, aggressive, audit_heavy).
        #[arg(long, value_enum, default_value = "default")]
        policy: CompactPolicyArg,
    },

    /// Generate export files in the configured export directory.
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

    /// Display prioritized actionable items from repair-log, cross-link candidates, and git hotspots.
    Handoffs {
        /// Limit to top N items.
        #[arg(long, short)]
        limit: Option<usize>,
        /// Only include items from the last N days.
        #[arg(long)]
        since: Option<u32>,
        /// Emit JSON instead of markdown table.
        #[arg(long)]
        json: bool,
    },

    #[command(name = "watch-internal", hide = true)]
    WatchInternal,

    /// Report only components whose status is not healthy. Exits non-zero if
    /// any degraded component is found.
    ///
    /// Narrow aggregation view over the same status snapshot used by
    /// `synrepo status` and the dashboard. Intended for CI hooks and
    /// pre-commit checks where a process-level failure is the signal.
    Doctor {
        /// Emit structured JSON instead of the compact text report.
        #[arg(long)]
        json: bool,
    },

    /// Open the guided operator dashboard.
    Dashboard,

    /// Start the optional HTTP metrics server.
    Server {
        /// Metrics bind address.
        #[arg(long, default_value = "127.0.0.1:9090")]
        metrics: String,
    },

    /// Start the MCP server over stdio.
    Mcp,

    /// Uninstall synrepo artifacts from the current repo.
    ///
    /// Bulk `synrepo remove` targets every tracked/detected agent skill or
    /// instructions file, the project's MCP entries, any root `.gitignore` line
    /// synrepo added, and prompts before deleting `.synrepo/` itself.
    /// `synrepo remove <tool>` narrows the plan to a single agent.
    /// `.mcp.json.bak` sidecars are never removed.
    Remove {
        /// Limit removal to a single agent's skill/instructions file + MCP entry.
        tool: Option<AgentTool>,
        /// Execute the plan. Without this flag, only a dry-run table is printed.
        #[arg(long)]
        apply: bool,
        /// Emit JSON instead of the human-readable plan / summary.
        #[arg(long)]
        json: bool,
        /// Skip the `.synrepo/` prompt and leave the directory in place.
        #[arg(long)]
        keep_synrepo_dir: bool,
        /// Non-interactive: answer "yes" to the `.synrepo/` prompt and
        /// proceed even when a watch daemon is still running.
        #[arg(long)]
        force: bool,
    },
}
