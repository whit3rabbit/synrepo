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
    structure::graph::{EdgeKind, Epistemic, GraphStore},
    surface::card::FileGitIntelligence,
    NodeId,
};

const FILE_NODE_GIT_INSIGHT_LIMIT: usize = 5;

pub(crate) fn graph_query_output(repo_root: &Path, query: &str) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    let parsed = parse_graph_query(query)?;
    let edges = match parsed.direction {
        QueryDirection::Outbound => store.outbound(parsed.node_id, parsed.edge_kind)?,
        QueryDirection::Inbound => store.inbound(parsed.node_id, parsed.edge_kind)?,
    };

    render_json(&GraphQueryOutput {
        direction: parsed.direction.as_str(),
        node_id: parsed.node_id.to_string(),
        edge_kind: parsed.edge_kind.map(|kind| kind.as_str().to_string()),
        edges: edges.into_iter().map(RenderedEdge::from).collect(),
    })
}

pub(crate) fn graph_stats_output(repo_root: &Path) -> anyhow::Result<String> {
    let store = open_graph_store_for_read(repo_root)?;
    render_json(&store.persisted_stats()?)
}

pub(crate) fn node_output(repo_root: &Path, id: &str) -> anyhow::Result<String> {
    let config = Config::load(repo_root)?;
    let store = open_graph_store_for_read(repo_root)?;
    let node_id = id.parse::<NodeId>()?;

    let payload = match node_id {
        NodeId::File(file_id) => store.get_file(file_id)?.map(|node| {
            let git_context = GitIntelligenceContext::inspect(repo_root, &config);
            let git_intelligence = FileGitIntelligence::from(analyze_path_history(
                &git_context,
                &node.path,
                config.git_commit_depth as usize,
                FILE_NODE_GIT_INSIGHT_LIMIT,
            )?);
            Ok::<_, anyhow::Error>(
                json!({ "node_id": id, "node_type": "file", "node": node, "git_intelligence": git_intelligence }),
            )
        }),
        NodeId::Symbol(symbol_id) => store
            .get_symbol(symbol_id)?
            .map(|node| Ok(json!({ "node_id": id, "node_type": "symbol", "node": node }))),
        NodeId::Concept(concept_id) => store
            .get_concept(concept_id)?
            .map(|node| Ok(json!({ "node_id": id, "node_type": "concept", "node": node }))),
    }
    .transpose()?
    .ok_or_else(|| anyhow::anyhow!("node not found: {id}"))?;

    render_json(&payload)
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
            anyhow::bail!(
                "Storage compatibility: {} requires {} because {}. Run `synrepo init` first.",
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
