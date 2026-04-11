//! synrepo CLI entry point.
//!
//! Phase 0/1 subcommands:
//! - `synrepo init [--mode auto|curated]` — create `.synrepo/` in the current repo
//! - `synrepo search <query>` — lexical search against the persisted index
//! - `synrepo graph query "<direction> <node_id> [edge_kind]"` — narrow graph traversal query (phase 1)
//! - `synrepo node <id>` — dump a node's metadata (phase 1)
//!
//! All non-trivial logic lives in the library crate. This file is dispatch only.

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use synrepo::config::{Config, Mode};
use synrepo::core::provenance::Provenance;
use synrepo::store::compatibility::{self, CompatAction, StoreId};
use synrepo::store::sqlite::SqliteGraphStore;
use synrepo::structure::graph::{EdgeKind, Epistemic, GraphStore};
use synrepo::NodeId;

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
    /// Run a narrow traversal query against the graph store.
    Query {
        /// Query syntax: `<direction> <node_id> [edge_kind]`.
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
    let output = graph_query_output(_repo_root, _q)?;
    println!("{output}");
    Ok(())
}

fn graph_stats(repo_root: &std::path::Path) -> anyhow::Result<()> {
    let output = graph_stats_output(repo_root)?;
    println!("{output}");
    Ok(())
}

fn node(repo_root: &std::path::Path, id: &str) -> anyhow::Result<()> {
    let output = node_output(repo_root, id)?;
    println!("{output}");
    Ok(())
}

fn graph_query_output(repo_root: &std::path::Path, q: &str) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    let query = parse_graph_query(q)?;
    let edges = match query.direction {
        QueryDirection::Outbound => store.outbound(query.node_id, query.edge_kind)?,
        QueryDirection::Inbound => store.inbound(query.node_id, query.edge_kind)?,
    };

    render_json(&GraphQueryOutput {
        direction: query.direction.as_str(),
        node_id: query.node_id.to_string(),
        edge_kind: query.edge_kind.map(|kind| kind.as_str().to_string()),
        edges: edges.into_iter().map(RenderedEdge::from).collect(),
    })
}

fn graph_stats_output(repo_root: &std::path::Path) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    render_json(&store.persisted_stats()?)
}

fn node_output(repo_root: &std::path::Path, id: &str) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    let node_id = id.parse::<NodeId>()?;

    let payload = match node_id {
        NodeId::File(file_id) => store
            .get_file(file_id)?
            .map(|node| json!({ "node_id": id, "node_type": "file", "node": node })),
        NodeId::Symbol(symbol_id) => store
            .get_symbol(symbol_id)?
            .map(|node| json!({ "node_id": id, "node_type": "symbol", "node": node })),
        NodeId::Concept(concept_id) => store
            .get_concept(concept_id)?
            .map(|node| json!({ "node_id": id, "node_type": "concept", "node": node })),
    }
    .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))?;

    render_json(&payload)
}

fn open_graph_store_for_read(repo_root: &std::path::Path) -> anyhow::Result<SqliteGraphStore> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let compatibility_report =
        compatibility::evaluate_runtime(&synrepo_dir, synrepo_dir.exists(), &config)?;
    if let Some(entry) = compatibility_report.entry_for(StoreId::Graph) {
        if entry.action != CompatAction::Continue {
            anyhow::bail!(
                "Storage compatibility: {} requires {} because {}.",
                entry.store_id.as_str(),
                entry.action.as_str(),
                entry.reason
            );
        }
    }

    let graph_dir = synrepo_dir.join("graph");
    if !SqliteGraphStore::db_path(&graph_dir).exists() {
        anyhow::bail!(
            "graph store is not materialized yet at {}",
            SqliteGraphStore::db_path(&graph_dir).display()
        );
    }

    Ok(SqliteGraphStore::open_existing(&graph_dir)?)
}

fn render_json<T: Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

#[derive(Clone, Copy, Debug)]
enum QueryDirection {
    Inbound,
    Outbound,
}

impl QueryDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GraphQuery {
    direction: QueryDirection,
    node_id: NodeId,
    edge_kind: Option<EdgeKind>,
}

#[derive(Serialize)]
struct GraphQueryOutput {
    direction: &'static str,
    node_id: String,
    edge_kind: Option<String>,
    edges: Vec<RenderedEdge>,
}

#[derive(Serialize)]
struct RenderedEdge {
    id: String,
    from: String,
    to: String,
    kind: String,
    epistemic: Epistemic,
    drift_score: f32,
    provenance: Provenance,
}

impl From<synrepo::structure::graph::Edge> for RenderedEdge {
    fn from(edge: synrepo::structure::graph::Edge) -> Self {
        Self {
            id: edge.id.to_string(),
            from: edge.from.to_string(),
            to: edge.to.to_string(),
            kind: edge.kind.as_str().to_string(),
            epistemic: edge.epistemic,
            drift_score: edge.drift_score,
            provenance: edge.provenance,
        }
    }
}

fn parse_graph_query(q: &str) -> anyhow::Result<GraphQuery> {
    let parts = q.split_whitespace().collect::<Vec<_>>();
    if !(parts.len() == 2 || parts.len() == 3) {
        anyhow::bail!(
            "invalid graph query: expected `<direction> <node_id> [edge_kind]`, got `{q}`"
        );
    }

    let direction = match parts[0] {
        "inbound" => QueryDirection::Inbound,
        "outbound" => QueryDirection::Outbound,
        other => anyhow::bail!("invalid graph query direction: {other}"),
    };
    let node_id = parts[1].parse::<NodeId>()?;
    let edge_kind = parts
        .get(2)
        .map(|kind| kind.parse::<EdgeKind>())
        .transpose()?;

    Ok(GraphQuery {
        direction,
        node_id,
        edge_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use synrepo::bootstrap::bootstrap;
    use synrepo::config::Config;
    use synrepo::core::ids::{ConceptNodeId, EdgeId, FileNodeId, SymbolNodeId};
    use synrepo::core::provenance::{CreatedBy, Provenance, SourceRef};
    use synrepo::structure::graph::{
        ConceptNode, EdgeKind, Epistemic, FileNode, SymbolKind, SymbolNode,
    };
    use tempfile::tempdir;
    use time::OffsetDateTime;

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

    #[test]
    fn node_output_returns_persisted_node_json() {
        let repo = tempdir().unwrap();
        let ids = seed_graph(repo.path());

        let output = node_output(repo.path(), &ids.file_id.to_string()).unwrap();
        let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

        assert_eq!(json["node_id"], ids.file_id.to_string());
        assert_eq!(json["node_type"], "file");
        assert_eq!(json["node"]["path"], "src/lib.rs");
        assert_eq!(json["node"]["provenance"]["pass"], "parse_code");
    }

    #[test]
    fn graph_stats_output_counts_persisted_rows() {
        let repo = tempdir().unwrap();
        seed_graph(repo.path());

        let output = graph_stats_output(repo.path()).unwrap();
        let json = serde_json::from_str::<serde_json::Value>(&output).unwrap();

        assert_eq!(json["file_nodes"], 1);
        assert_eq!(json["symbol_nodes"], 1);
        assert_eq!(json["concept_nodes"], 1);
        assert_eq!(json["total_edges"], 2);
        assert_eq!(json["edge_counts_by_kind"]["defines"], 1);
        assert_eq!(json["edge_counts_by_kind"]["governs"], 1);
    }

    #[test]
    fn graph_query_output_traverses_edges_with_optional_kind_filter() {
        let repo = tempdir().unwrap();
        let ids = seed_graph(repo.path());

        let outbound =
            graph_query_output(repo.path(), &format!("outbound {} defines", ids.file_id)).unwrap();
        let outbound_json = serde_json::from_str::<serde_json::Value>(&outbound).unwrap();

        assert_eq!(outbound_json["direction"], "outbound");
        assert_eq!(outbound_json["node_id"], ids.file_id.to_string());
        assert_eq!(outbound_json["edge_kind"], "defines");
        assert_eq!(outbound_json["edges"].as_array().unwrap().len(), 1);
        assert_eq!(outbound_json["edges"][0]["kind"], "defines");
        assert_eq!(outbound_json["edges"][0]["id"], "edge_0000000000000077");
        assert_eq!(outbound_json["edges"][0]["from"], ids.file_id.to_string());
        assert_eq!(outbound_json["edges"][0]["to"], ids.symbol_id.to_string());

        let inbound =
            graph_query_output(repo.path(), &format!("inbound {} governs", ids.file_id)).unwrap();
        let inbound_json = serde_json::from_str::<serde_json::Value>(&inbound).unwrap();

        assert_eq!(inbound_json["direction"], "inbound");
        assert_eq!(inbound_json["edge_kind"], "governs");
        assert_eq!(inbound_json["edges"].as_array().unwrap().len(), 1);
        assert_eq!(inbound_json["edges"][0]["from"], ids.concept_id.to_string());
    }

    struct SeededGraphIds {
        file_id: FileNodeId,
        symbol_id: SymbolNodeId,
        concept_id: ConceptNodeId,
    }

    fn seed_graph(repo_root: &std::path::Path) -> SeededGraphIds {
        bootstrap(repo_root, None).unwrap();

        let graph_dir = Config::synrepo_dir(repo_root).join("graph");
        let mut store = SqliteGraphStore::open(&graph_dir).unwrap();
        let file_id = FileNodeId(0x42);
        let symbol_id = SymbolNodeId(0x24);
        let concept_id = ConceptNodeId(0x99);

        store
            .upsert_file(FileNode {
                id: file_id,
                path: "src/lib.rs".to_string(),
                path_history: vec!["src/old_lib.rs".to_string()],
                content_hash: "abc123".to_string(),
                size_bytes: 128,
                language: Some("rust".to_string()),
                epistemic: Epistemic::ParserObserved,
                provenance: sample_provenance("parse_code", "src/lib.rs"),
            })
            .unwrap();
        store
            .upsert_symbol(SymbolNode {
                id: symbol_id,
                file_id,
                qualified_name: "synrepo::lib".to_string(),
                display_name: "lib".to_string(),
                kind: SymbolKind::Module,
                body_byte_range: (0, 64),
                body_hash: "def456".to_string(),
                signature: Some("pub mod lib".to_string()),
                doc_comment: None,
                epistemic: Epistemic::ParserObserved,
                provenance: sample_provenance("parse_code", "src/lib.rs"),
            })
            .unwrap();
        store
            .upsert_concept(ConceptNode {
                id: concept_id,
                path: "docs/adr/0001-graph.md".to_string(),
                title: "Graph Storage".to_string(),
                aliases: vec!["canonical-graph".to_string()],
                summary: Some("Why the graph stays observed-only.".to_string()),
                epistemic: Epistemic::HumanDeclared,
                provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
            })
            .unwrap();
        store
            .insert_edge(synrepo::structure::graph::Edge {
                id: EdgeId(0x77),
                from: NodeId::File(file_id),
                to: NodeId::Symbol(symbol_id),
                kind: EdgeKind::Defines,
                epistemic: Epistemic::ParserObserved,
                drift_score: 0.0,
                provenance: sample_provenance("resolve_edges", "src/lib.rs"),
            })
            .unwrap();
        store
            .insert_edge(synrepo::structure::graph::Edge {
                id: EdgeId(0x78),
                from: NodeId::Concept(concept_id),
                to: NodeId::File(file_id),
                kind: EdgeKind::Governs,
                epistemic: Epistemic::HumanDeclared,
                drift_score: 0.2,
                provenance: sample_provenance("parse_prose", "docs/adr/0001-graph.md"),
            })
            .unwrap();
        store.commit().unwrap();

        SeededGraphIds {
            file_id,
            symbol_id,
            concept_id,
        }
    }

    fn sample_provenance(pass: &str, path: &str) -> Provenance {
        Provenance {
            created_at: OffsetDateTime::UNIX_EPOCH,
            source_revision: "deadbeef".to_string(),
            created_by: CreatedBy::StructuralPipeline,
            pass: pass.to_string(),
            source_artifacts: vec![SourceRef {
                file_id: None,
                path: path.to_string(),
                content_hash: "hash".to_string(),
            }],
        }
    }
}
