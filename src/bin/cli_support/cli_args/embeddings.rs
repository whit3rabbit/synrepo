use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum EmbeddingsCommand {
    /// Build or rebuild the optional embedding vector index.
    Build {
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}
