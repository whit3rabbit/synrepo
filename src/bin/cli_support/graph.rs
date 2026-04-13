use std::path::Path;

use serde::Serialize;
use serde_json::json;
use synrepo::{
    config::Config,
    core::provenance::Provenance,
    pipeline::{git::GitIntelligenceContext, git_intelligence::analyze_path_history},
    store::{
        compatibility::{self, CompatAction, StoreId},
        sqlite::SqliteGraphStore,
    },
    structure::graph::{with_graph_read_snapshot, EdgeKind, Epistemic},
    surface::card::FileGitIntelligence,
    NodeId,
};

const FILE_NODE_GIT_INSIGHT_LIMIT: usize = 5;

/// Query the graph by direction, node ID, and optional edge kind.
pub(crate) fn graph_query_output(repo_root: &Path, query: &str) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    let parsed = parse_graph_query(query)?;
    let edges = with_graph_read_snapshot(&store, |graph| match parsed.direction {
        QueryDirection::Outbound => graph.outbound(parsed.node_id, parsed.edge_kind),
        QueryDirection::Inbound => graph.inbound(parsed.node_id, parsed.edge_kind),
    })?;

    render_json(&GraphQueryOutput {
        direction: parsed.direction.as_str(),
        node_id: parsed.node_id.to_string(),
        edge_kind: parsed.edge_kind.map(|kind| kind.as_str().to_string()),
        edges: edges.into_iter().map(RenderedEdge::from).collect(),
    })
}

/// Retrieve statistics for the currently persisted graph store.
pub(crate) fn graph_stats_output(repo_root: &Path) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    // persisted_stats issues COUNT(*) against four tables plus a GROUP BY,
    // so wrap it in one snapshot to avoid mixing counts from different epochs.
    // persisted_stats is inherent on SqliteGraphStore, not a trait method, so
    // we ignore the trait-object parameter and call through the concrete type.
    let stats = with_graph_read_snapshot(&store, |_graph| store.persisted_stats())?;
    render_json(&stats)
}

/// Retrieve the full JSON output of a specific node by ID.
pub(crate) fn node_output(repo_root: &Path, id: &str) -> anyhow::Result<String> {
    let config = Config::load(repo_root)?;
    let store = open_graph_store_for_read(repo_root)?;
    let node_id = id.parse::<NodeId>()?;

    // Read the graph node inside a single snapshot. Git intelligence reads
    // the on-disk repo rather than the graph, so it can run after the
    // snapshot ends without reintroducing a consistency hazard.
    let node_fetch: Option<NodePayload> = with_graph_read_snapshot(&store, |graph| {
        Ok(match node_id {
            NodeId::File(file_id) => graph.get_file(file_id)?.map(|node| NodePayload::File {
                path: node.path.clone(),
                node,
            }),
            NodeId::Symbol(symbol_id) => graph.get_symbol(symbol_id)?.map(NodePayload::Symbol),
            NodeId::Concept(concept_id) => graph.get_concept(concept_id)?.map(NodePayload::Concept),
        })
    })?;

    let payload = match node_fetch {
        Some(NodePayload::File { path, node }) => {
            let git_context = GitIntelligenceContext::inspect(repo_root, &config);
            let git_intelligence = FileGitIntelligence::from(analyze_path_history(
                &git_context,
                &path,
                config.git_commit_depth as usize,
                FILE_NODE_GIT_INSIGHT_LIMIT,
            )?);
            json!({ "node_id": id, "node_type": "file", "node": node, "git_intelligence": git_intelligence })
        }
        Some(NodePayload::Symbol(node)) => {
            json!({ "node_id": id, "node_type": "symbol", "node": node })
        }
        Some(NodePayload::Concept(node)) => {
            json!({ "node_id": id, "node_type": "concept", "node": node })
        }
        None => return Err(anyhow::anyhow!("node not found: {id}")),
    };

    render_json(&payload)
}

enum NodePayload {
    File {
        path: String,
        node: synrepo::structure::graph::FileNode,
    },
    Symbol(synrepo::structure::graph::SymbolNode),
    Concept(synrepo::structure::graph::ConceptNode),
}

/// Verify that a store is in a compatible state before operating on it.
pub(crate) fn check_store_ready(
    synrepo_dir: &Path,
    config: &Config,
    store: StoreId,
) -> anyhow::Result<()> {
    let report = compatibility::evaluate_runtime(synrepo_dir, synrepo_dir.exists(), config)?;
    if let Some(entry) = report.entry_for(store) {
        if entry.action != CompatAction::Continue {
            let hint = match entry.action {
                CompatAction::Block | CompatAction::MigrateRequired => {
                    "Run `synrepo upgrade` to see recovery steps."
                }
                _ => "Run `synrepo upgrade --apply` to resolve, or `synrepo init` to reinitialize.",
            };
            anyhow::bail!(
                "Storage compatibility: {} requires {} because {}. {hint}",
                entry.store_id.as_str(),
                entry.action.as_str(),
                entry.reason
            );
        }
    }
    Ok(())
}

fn open_graph_store_for_read(repo_root: &Path) -> anyhow::Result<SqliteGraphStore> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    check_store_ready(&synrepo_dir, &config, StoreId::Graph)?;

    let graph_dir = synrepo_dir.join("graph");
    let db_path = SqliteGraphStore::db_path(&graph_dir);
    if !db_path.exists() {
        anyhow::bail!(
            "graph store is not materialized yet at {}",
            db_path.display()
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

fn parse_graph_query(query: &str) -> anyhow::Result<GraphQuery> {
    let parts = query.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 || parts.len() > 3 {
        anyhow::bail!(
            "invalid graph query: expected `<direction> <node_id> [edge_kind]`, got `{query}`"
        );
    }

    let direction = match parts[0] {
        "inbound" => QueryDirection::Inbound,
        "outbound" => QueryDirection::Outbound,
        other => anyhow::bail!("invalid graph query direction: {other}"),
    };

    Ok(GraphQuery {
        direction,
        node_id: parts[1].parse::<NodeId>()?,
        edge_kind: parts
            .get(2)
            .map(|kind| kind.parse::<EdgeKind>())
            .transpose()?,
    })
}
