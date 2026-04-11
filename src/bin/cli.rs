//! synrepo CLI entry point.
//!
//! Phase 0/1 subcommands:
//! - `synrepo init [--mode auto|curated]` — create `.synrepo/` in the current repo
//! - `synrepo search <query>` — lexical search against the persisted index
//! - `synrepo graph query <q>` — structured graph query (phase 1)
//! - `synrepo node <id>` — dump a node's metadata (phase 1)
//!
//! All non-trivial logic lives in the library crate. This file is dispatch only.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use synrepo::config::{Config, Mode};
use synrepo::store::compatibility::{self, CompatAction, StoreId};

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
        /// The node ID in display format (e.g. `file_0000000000000042`).
        id: String,
    },
}

#[derive(Subcommand)]
enum GraphCommand {
    /// Run a structured query against the graph store.
    Query {
        /// The query string.
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
    fn from(m: ModeArg) -> Self {
        match m {
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
    let repo_root = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    match cli.command {
        Command::Init { mode } => init(&repo_root, mode.map(Into::into)),
        Command::Search { query } => search(&repo_root, &query),
        Command::Graph(GraphCommand::Query { q }) => graph_query(&repo_root, &q),
        Command::Graph(GraphCommand::Stats) => graph_stats(&repo_root),
        Command::Node { id } => node(&repo_root, &id),
    }
}

fn init(repo_root: &std::path::Path, requested_mode: Option<Mode>) -> anyhow::Result<()> {
    let report = synrepo::bootstrap::bootstrap(repo_root, requested_mode)?;
    print!("{}", report.render());
    Ok(())
}

fn search(repo_root: &std::path::Path, query: &str) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let compatibility_report =
        compatibility::evaluate_runtime(&synrepo_dir, synrepo_dir.exists(), &config)?;
    if let Some(entry) = compatibility_report.entry_for(StoreId::Index) {
        if entry.action != CompatAction::Continue {
            anyhow::bail!(
                "Storage compatibility: {} requires {} because {}. Run `synrepo init` first.",
                entry.store_id.as_str(),
                entry.action.as_str(),
                entry.reason
            );
        }
    }

    let matches = synrepo::substrate::search(&config, repo_root, query)?;

    for m in &matches {
        println!(
            "{}:{}: {}",
            m.path.display(),
            m.line_number,
            String::from_utf8_lossy(&m.line_content).trim_end()
        );
    }

    println!("Found {} matches.", matches.len());
    Ok(())
}

fn graph_query(_repo_root: &std::path::Path, _q: &str) -> anyhow::Result<()> {
    // TODO(phase-1)
    anyhow::bail!("graph query not yet implemented (phase 1 pending)")
}

fn graph_stats(_repo_root: &std::path::Path) -> anyhow::Result<()> {
    // TODO(phase-1)
    anyhow::bail!("graph stats not yet implemented (phase 1 pending)")
}

fn node(_repo_root: &std::path::Path, _id: &str) -> anyhow::Result<()> {
    // TODO(phase-1)
    anyhow::bail!("node lookup not yet implemented (phase 1 pending)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use synrepo::bootstrap::bootstrap;
    use synrepo::config::Config;
    use tempfile::tempdir;

    #[test]
    fn search_requires_rebuild_when_index_sensitive_config_changes() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "search token\n").unwrap();
        bootstrap(repo.path(), None).unwrap();

        let updated = Config {
            roots: vec!["src".to_string()],
            ..Config::load(repo.path()).unwrap()
        };
        std::fs::write(
            Config::synrepo_dir(repo.path()).join("config.toml"),
            toml::to_string_pretty(&updated).unwrap(),
        )
        .unwrap();

        let error = search(repo.path(), "search token").unwrap_err().to_string();

        assert!(error.contains("Storage compatibility"));
        assert!(error.contains("requires rebuild"));
    }
}
