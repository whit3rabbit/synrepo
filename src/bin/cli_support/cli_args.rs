//! CLI argument types for synrepo.
//!
//! Pure declarative clap derives with zero runtime logic.
//! The dispatcher lives in `cli.rs`.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use synrepo::config::Mode;
use synrepo::pipeline::export::ExportFormat;
use synrepo::pipeline::maintenance::CompactPolicy;

use super::agent_shims::AgentTool;

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
    ///
    /// Reads only; never acquires the writer lock or mutates any store.
    /// Safe to run at any time, including while a reconcile is in progress.
    /// Use this as the quick "is synrepo healthy?" check.
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
    ///
    /// Writes a named fragment file and prints the one-line include instruction.
    /// Never modifies existing configuration files. Use `--force` to overwrite,
    /// or `--regen` to compare and overwrite only if the content has changed.
    ///
    /// For a full onboarding flow (init + skill/instructions + MCP registration),
    /// use `synrepo setup`.
    AgentSetup {
        /// Target agent CLI.
        tool: AgentTool,
        /// Overwrite an existing skill or instructions file if one already exists.
        #[arg(long)]
        force: bool,
        /// Compare existing file against the current template; overwrite if different.
        #[arg(long)]
        regen: bool,
    },

    /// Set up synrepo for this repo and wire an agent.
    ///
    /// This is the recommended first-run command. With a `<tool>` argument it
    /// runs the scripted flow: `synrepo init`, writes the client-specific skill
    /// or instructions file, and registers the synrepo MCP server in the
    /// project's local configuration when that integration is automated.
    /// Without a `<tool>` argument it launches the interactive TUI wizard,
    /// which prompts for repo mode, agent target, and optional commentary
    /// provider before applying the same steps. The `--force`, `--synthesis`,
    /// and `--gitignore` flags only apply to the scripted flow; passing them
    /// without a tool is rejected.
    Setup {
        /// Target client to set up. Omit to launch the interactive wizard.
        tool: Option<AgentTool>,
        /// Force re-initialization and overwrite existing configs.
        #[arg(long)]
        force: bool,
        /// After the normal setup steps complete, launch the synthesis sub-wizard
        /// and patch repo-local `.synrepo/config.toml` plus user-scoped
        /// `~/.synrepo/config.toml` as needed.
        /// Off by default; opt-in makes the key-detected hint in `synrepo status`
        /// actionable without requiring the user to hand-edit config.
        #[arg(long)]
        synthesis: bool,
        /// Add .synrepo/ to the root .gitignore file.
        #[arg(long)]
        gitignore: bool,
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
        /// Rotate the synthesis call log and zero the per-repo totals snapshot.
        /// Does not run repair; returns after the reset.
        #[arg(long)]
        reset_synthesis_totals: bool,
    },

    /// Refresh advisory commentary for missing or stale rows, optionally scoped to paths.
    ///
    /// With no arguments or flags, generates commentary for all graph nodes
    /// that lack a machine-authored summary, then refreshes any stale entries
    /// (same as the `RefreshCommentary` repair action in `synrepo sync`).
    /// Positional `<paths>` scopes the run to files whose path starts with one
    /// of the given prefixes. `--changed` derives the scope from hotspots in
    /// the last 50 commits via git intelligence. `--dry-run` prints the
    /// target set without calling any provider.
    #[command(alias = "synthesize")]
    Synthesis {
        /// Repo-root-relative path prefixes to scope to. Empty = all stale.
        paths: Vec<String>,
        /// Use recent-commit hotspots as the scope (ignores positional paths).
        #[arg(long)]
        changed: bool,
        /// Print the planned target set without calling any provider.
        #[arg(long)]
        dry_run: bool,
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

    /// Compact overlay, state, and index stores to reclaim disk space.
    ///
    /// Dry-run by default: prints a plan (compactable counts by component) and exits.
    /// Pass `--apply` to execute the compaction actions: compact stale commentary,
    /// summarize old cross-link audit rows, rotate the repair-log, run WAL checkpoint,
    /// and optionally rebuild the index.
    Compact {
        /// Execute the compaction actions instead of printing a dry-run plan.
        #[arg(long)]
        apply: bool,
        /// Retention policy preset (default, aggressive, audit_heavy).
        #[arg(long, value_enum, default_value = "default")]
        policy: CompactPolicyArg,
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

    /// Open the guided operator dashboard.
    ///
    /// Explicit alias for bare `synrepo` on a ready repo. Use it to inspect
    /// health, watch activity, commentary usage, and next actions. Exits
    /// non-zero with a pointer to the correct subcommand when the repo is
    /// uninitialized or partial, instead of routing to the setup or repair
    /// wizard.
    Dashboard,

    /// Start the MCP server over stdio.
    ///
    /// Exposes 16 read-only tools for coding agents to query the repository's
    /// structural graph, cards, search index, overlay data, and provenance.
    /// Communicates over stdio using the MCP protocol.
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
        /// Maximum number of candidates to return (default 50; pass 0 to disable the cap).
        #[arg(long)]
        limit: Option<usize>,
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

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum CompactPolicyArg {
    Default,
    Aggressive,
    AuditHeavy,
}

impl From<CompactPolicyArg> for CompactPolicy {
    fn from(arg: CompactPolicyArg) -> Self {
        match arg {
            CompactPolicyArg::Default => CompactPolicy::Default,
            CompactPolicyArg::Aggressive => CompactPolicy::Aggressive,
            CompactPolicyArg::AuditHeavy => CompactPolicy::AuditHeavy,
        }
    }
}
