//! Secondary subcommand enums used by `Command`.

use clap::Subcommand;

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
pub(crate) enum GraphCommand {
    /// Run a narrow traversal query against the graph store.
    Query {
        /// Query syntax: `<direction> <node_id> [edge_kind]`.
        q: String,
    },

    /// Print graph statistics (node count by type, edge count by kind).
    Stats,
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
