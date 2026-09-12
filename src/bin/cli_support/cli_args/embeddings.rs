use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum EmbeddingsCommand {
    /// Build or rebuild the optional embedding vector index.
    Build {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Remove vector-index artifacts that no longer match the active config:
    /// stale profile subdirectories and the legacy flat v5 `index.bin`.
    Clean {
        /// Apply the deletion. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}
