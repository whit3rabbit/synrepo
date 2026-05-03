//! Secondary subcommand enums used by `Command`.

use clap::{Args, Subcommand};
use std::path::PathBuf;

use crate::cli_support::agent_shims::AgentTool;

#[derive(Args)]
pub(crate) struct AgentSetupArgs {
    /// Target agent CLI. Omit when using `--only` or `--skip`.
    #[arg(conflicts_with_all = ["only", "skip"])]
    pub(crate) tool: Option<AgentTool>,
    /// Comma-separated list of targets to set up. Mutually exclusive
    /// with the positional `tool` argument and with `--skip`.
    #[arg(long, value_delimiter = ',', conflicts_with = "skip")]
    pub(crate) only: Vec<AgentTool>,
    /// Apply to every known target except these. Comma-separated.
    /// Mutually exclusive with the positional `tool` argument and with `--only`.
    #[arg(long, value_delimiter = ',', conflicts_with = "only")]
    pub(crate) skip: Vec<AgentTool>,
    /// Overwrite an existing skill or instructions file if one already exists.
    #[arg(long)]
    pub(crate) force: bool,
    /// Compare existing file against the current template; overwrite if different.
    #[arg(long)]
    pub(crate) regen: bool,
}

#[derive(Args)]
pub(crate) struct SetupArgs {
    /// Target client to set up. Omit to launch the interactive wizard,
    /// or pair with `--only`/`--skip` for multi-client setup.
    #[arg(conflicts_with_all = ["only", "skip"])]
    pub(crate) tool: Option<AgentTool>,
    /// Comma-separated list of targets to set up in one pass. Mutually
    /// exclusive with the positional `tool` argument and with `--skip`.
    #[arg(long, value_delimiter = ',', conflicts_with = "skip")]
    pub(crate) only: Vec<AgentTool>,
    /// Apply setup to every known target except these. Comma-separated.
    /// Mutually exclusive with the positional `tool` argument and `--only`.
    #[arg(long, value_delimiter = ',', conflicts_with = "only")]
    pub(crate) skip: Vec<AgentTool>,
    /// Force re-initialization and overwrite existing configs.
    #[arg(long)]
    pub(crate) force: bool,
    /// After normal setup, launch the explain sub-wizard and patch config.
    #[arg(long)]
    pub(crate) explain: bool,
    /// Add .synrepo/ to the root .gitignore file.
    #[arg(long)]
    pub(crate) gitignore: bool,
    /// Configure the MCP server in this project instead of user-global config.
    #[arg(long, conflicts_with = "global")]
    pub(crate) project: bool,
    /// Deprecated no-op: global setup is now the default.
    #[arg(long, hide = true, conflicts_with = "project")]
    pub(crate) global: bool,
}

#[derive(Args)]
pub(crate) struct UninstallArgs {
    /// Execute the plan. Without this flag, non-TTY output is a dry run.
    #[arg(long)]
    pub(crate) apply: bool,
    /// Emit JSON instead of the human-readable plan / summary.
    #[arg(long)]
    pub(crate) json: bool,
    /// Non-interactive: apply selected actions and override watch-daemon blocks.
    #[arg(long)]
    pub(crate) force: bool,
    /// Select database/cache deletion rows in non-interactive runs.
    #[arg(long)]
    pub(crate) delete_data: bool,
    /// Keep the synrepo binary even when direct deletion is safe.
    #[arg(long)]
    pub(crate) keep_binary: bool,
}

#[derive(Args)]
pub(crate) struct CiRunArgs {
    /// Target file path, node ID, or symbol name. Repeat to include several cards.
    #[arg(long = "target")]
    pub(crate) targets: Vec<String>,
    /// Add changed files from `git diff --name-only <ref>...HEAD`.
    #[arg(long)]
    pub(crate) changed_from: Option<String>,
    /// Budget tier: tiny, normal, or deep. Defaults to tiny.
    #[arg(long, short)]
    pub(crate) budget: Option<String>,
    /// Emit JSON instead of markdown text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Subcommand)]
pub(crate) enum WatchCommand {
    /// Show watch-service status for the current repo.
    Status,
    /// Stop the active watch service for the current repo.
    Stop,
}

#[derive(Subcommand)]
pub(crate) enum ProjectCommand {
    /// Register a repository as a managed project.
    Add {
        /// Repository path. Defaults to the current repo root.
        path: Option<PathBuf>,
    },
    /// List managed projects and their current health.
    List {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Inspect one managed project.
    Inspect {
        /// Repository path. Defaults to the current repo root.
        path: Option<PathBuf>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Unregister a managed project without deleting repository state.
    Remove {
        /// Repository path. Defaults to the current repo root.
        path: Option<PathBuf>,
    },
    /// Report stale managed projects. Dry-run unless --apply is passed.
    PruneMissing {
        #[arg(long, help = "Unregister missing projects from the global registry")]
        apply: bool,
        #[arg(long, help = "Emit a machine-readable cleanup report")]
        json: bool,
    },
    /// Resolve and mark a managed project as recently used.
    Use { selector: String },
    /// Rename a managed project's display alias.
    Rename { selector: String, name: String },
}

#[derive(Subcommand)]
pub(crate) enum DocsCommand {
    /// Materialize editable explain docs from overlay commentary.
    Export {
        /// Rebuild explain-docs and explain-index before exporting. This
        /// discards unimported Markdown edits but leaves overlay commentary
        /// untouched.
        #[arg(long)]
        force: bool,
    },
    /// Remove materialized explain docs and their search index.
    Clean {
        /// Apply the deletion. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,
    },
    /// List materialized explain docs.
    List,
    /// Search materialized explain docs.
    Search {
        /// Lexical query string.
        query: String,
        /// Maximum results to return.
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Import edited explain-doc bodies back into the overlay.
    Import {
        /// Import every materialized commentary doc.
        #[arg(long, conflicts_with = "path")]
        all: bool,
        /// Path to one materialized commentary doc.
        path: Option<PathBuf>,
    },
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
pub(crate) enum NotesCommand {
    /// Add an advisory note.
    Add {
        /// Target kind: path, file, symbol, concept, test, card, note.
        #[arg(long)]
        target_kind: String,
        /// Target ID or repo-relative path.
        #[arg(long)]
        target: String,
        /// Advisory claim.
        #[arg(long)]
        claim: String,
        /// Author/tool identity.
        #[arg(long, default_value = "cli-user")]
        created_by: String,
        /// Confidence: low, medium, high.
        #[arg(long, default_value = "medium")]
        confidence: String,
        /// JSON array of evidence objects: [{"kind":"symbol","id":"sym_..."}].
        #[arg(long)]
        evidence_json: Option<String>,
        /// JSON array of source hash anchors: [{"path":"src/lib.rs","hash":"..."}].
        #[arg(long)]
        source_hashes_json: Option<String>,
        /// Optional graph revision anchor.
        #[arg(long)]
        graph_revision: Option<u64>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// List advisory notes. Forgotten, superseded, and invalid notes are hidden by default.
    List {
        /// Optional target kind filter.
        #[arg(long)]
        target_kind: Option<String>,
        /// Optional target ID or path filter.
        #[arg(long)]
        target: Option<String>,
        /// Maximum notes to return.
        #[arg(long)]
        limit: Option<usize>,
        /// Include forgotten, superseded, and invalid notes.
        #[arg(long)]
        include_all: bool,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Audit notes, including hidden lifecycle states.
    Audit {
        /// Optional target kind filter.
        #[arg(long)]
        target_kind: Option<String>,
        /// Optional target ID or path filter.
        #[arg(long)]
        target: Option<String>,
        /// Maximum notes to return.
        #[arg(long)]
        limit: Option<usize>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Link two notes.
    Link {
        /// Source note ID.
        from_note: String,
        /// Target note ID.
        to_note: String,
        /// Actor identity.
        #[arg(long, default_value = "cli-user")]
        actor: String,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Supersede a note with a replacement claim.
    Supersede {
        /// Old note ID.
        old_note: String,
        /// Replacement target kind.
        #[arg(long)]
        target_kind: String,
        /// Replacement target ID or repo-relative path.
        #[arg(long)]
        target: String,
        /// Replacement advisory claim.
        #[arg(long)]
        claim: String,
        /// Author/tool identity.
        #[arg(long, default_value = "cli-user")]
        created_by: String,
        /// Confidence: low, medium, high.
        #[arg(long, default_value = "medium")]
        confidence: String,
        /// JSON array of evidence objects.
        #[arg(long)]
        evidence_json: Option<String>,
        /// JSON array of source hash anchors.
        #[arg(long)]
        source_hashes_json: Option<String>,
        /// Optional graph revision anchor.
        #[arg(long)]
        graph_revision: Option<u64>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Forget a note, hiding it from normal retrieval.
    Forget {
        /// Note ID.
        note_id: String,
        /// Actor identity.
        #[arg(long, default_value = "cli-user")]
        actor: String,
        /// Optional reason.
        #[arg(long)]
        reason: Option<String>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Verify a note and return it to active state.
    Verify {
        /// Note ID.
        note_id: String,
        /// Actor identity.
        #[arg(long, default_value = "cli-user")]
        actor: String,
        /// Optional graph revision anchor.
        #[arg(long)]
        graph_revision: Option<u64>,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum StatsCommand {
    /// Context-serving metrics.
    Context {
        /// Output format (text, json, prometheus). Defaults to text.
        /// Mutually exclusive with `--json`.
        #[arg(long, value_enum)]
        format: Option<super::StatFormatArg>,
        /// Emit JSON instead of human-readable output.
        /// Alias for `--format json` kept for back-compat.
        #[arg(long, conflicts_with = "format")]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum BenchCommand {
    /// Benchmark context savings and target hit rate.
    Context {
        /// Glob for JSON task fixtures.
        #[arg(long)]
        tasks: String,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}
