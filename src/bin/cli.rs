//! synrepo CLI entry point.
//!
//! Phase 0/1 subcommands:
//! - `synrepo init [--mode auto|curated]` — create `.synrepo/` in the current repo
//! - `synrepo search <query>` — lexical search via syntext (phase 0)
//! - `synrepo graph query <q>` — structured graph query (phase 1)
//! - `synrepo node <id>` — dump a node's metadata (phase 1)
//!
//! Card-returning subcommands (`synrepo card`, `synrepo where-to-edit`, etc.)
//! land in phase 2 alongside the MCP server.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use synrepo::config::{Config, Mode};

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
        #[arg(long, value_enum, default_value_t = ModeArg::Auto)]
        mode: ModeArg,
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
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();
    let repo_root = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    match cli.command {
        Command::Init { mode } => init(&repo_root, mode.into()),
        Command::Search { query } => search(&repo_root, &query),
        Command::Graph(GraphCommand::Query { q }) => graph_query(&repo_root, &q),
        Command::Graph(GraphCommand::Stats) => graph_stats(&repo_root),
        Command::Node { id } => node(&repo_root, &id),
    }
}

fn init(repo_root: &std::path::Path, mode: Mode) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    if synrepo_dir.exists() {
        anyhow::bail!(".synrepo/ already exists at {:?}", synrepo_dir);
    }
    std::fs::create_dir_all(&synrepo_dir)?;
    std::fs::create_dir_all(synrepo_dir.join("graph"))?;
    std::fs::create_dir_all(synrepo_dir.join("overlay"))?;
    std::fs::create_dir_all(synrepo_dir.join("index"))?;
    std::fs::create_dir_all(synrepo_dir.join("embeddings"))?;
    std::fs::create_dir_all(synrepo_dir.join("cache/llm-responses"))?;
    std::fs::create_dir_all(synrepo_dir.join("state"))?;

    let config = Config {
        mode,
        ..Config::default()
    };
    let config_path = synrepo_dir.join("config.toml");
    std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;

    // Write a default .gitignore for .synrepo/
    let gitignore_path = synrepo_dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        "# Gitignore everything in .synrepo/ except config.toml\n\
         *\n\
         !.gitignore\n\
         !config.toml\n",
    )?;

    println!("Initialized .synrepo/ at {:?} in {:?} mode", synrepo_dir, mode);
    
    // Phase 0: run first structural compile (build syntext index)
    println!("Building initial substrate index...");
    synrepo::substrate::build_index(&config, repo_root)?;
    println!("Index build complete.");
    
    Ok(())
}

fn search(repo_root: &std::path::Path, query: &str) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let matches = synrepo::substrate::search(&config, repo_root, query)?;
    
    for m in &matches {
        println!("{}:{}: {}", m.path.display(), m.line_number, String::from_utf8_lossy(&m.line_content).trim_end());
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