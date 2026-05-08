use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum StatsCommand {
    /// Context-serving metrics.
    Context {
        /// Output format (text, json, prometheus). Defaults to text.
        /// Mutually exclusive with `--json`.
        #[arg(long, value_enum)]
        format: Option<super::super::StatFormatArg>,
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
        /// Benchmark mode: cards, ask, or all. Defaults to cards for v1 compatibility.
        #[arg(long, default_value = "cards")]
        mode: String,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Benchmark lexical versus hybrid search hit rate.
    Search {
        /// Glob for JSON task fixtures.
        #[arg(long)]
        tasks: String,
        #[arg(long, default_value = "both")]
        mode: String,
        /// Emit JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}
