use std::path::Path;

use crate::{
    core::ids::NodeId,
    store::{
        overlay::{
            parse_cross_link_freshness, parse_overlay_edge_kind, FindingsFilter, SqliteOverlayStore,
        },
        sqlite::SqliteGraphStore,
    },
    structure::graph::{snapshot, with_graph_read_snapshot},
};
use anyhow::Context as _;
use serde_json::json;

pub(crate) fn render_findings(
    repo_root: &Path,
    node_id: Option<String>,
    kind: Option<String>,
    freshness: Option<String>,
    limit: u32,
) -> anyhow::Result<serde_json::Value> {
    let synrepo_dir = crate::config::Config::synrepo_dir(repo_root);
    let graph_dir = synrepo_dir.join("graph");
    let overlay_dir = synrepo_dir.join("overlay");

    let overlay = SqliteOverlayStore::open_existing(&overlay_dir).with_context(|| {
        format!(
            "Overlay store not found at {} — generate cross-links first",
            overlay_dir.display()
        )
    })?;

    let node_id = node_id
        .map(|value| value.parse::<NodeId>())
        .transpose()
        .map_err(|error| anyhow::anyhow!("invalid node_id: {error}"))?;
    let kind = kind
        .as_deref()
        .map(parse_overlay_edge_kind)
        .transpose()
        .map_err(|error| anyhow::anyhow!("invalid kind: {error}"))?;
    let freshness = freshness
        .as_deref()
        .map(parse_cross_link_freshness)
        .transpose()
        .map_err(|error| anyhow::anyhow!("invalid freshness: {error}"))?;

    let filter = FindingsFilter {
        node_id,
        kind,
        freshness,
        limit: Some(limit as usize),
    };

    let findings = if super::graph_snapshot_disabled() {
        let graph = SqliteGraphStore::open_existing(&graph_dir).with_context(|| {
            format!(
                "Graph store not found at {} — run `synrepo init` first",
                graph_dir.display()
            )
        })?;
        with_graph_read_snapshot(&graph, |reader| overlay.findings(reader, &filter))?
    } else {
        let graph = snapshot::current();
        if graph.snapshot_epoch == 0 {
            let sqlite = SqliteGraphStore::open_existing(&graph_dir).with_context(|| {
                format!(
                    "Graph store not found at {} — run `synrepo init` first",
                    graph_dir.display()
                )
            })?;
            with_graph_read_snapshot(&sqlite, |reader| overlay.findings(reader, &filter))?
        } else {
            overlay.findings(graph.as_ref(), &filter)?
        }
    };

    Ok(json!({ "findings": findings }))
}
