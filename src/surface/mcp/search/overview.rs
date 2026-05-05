use serde_json::json;

use crate::{
    bootstrap::runtime_probe::probe,
    surface::{
        readiness::ReadinessMatrix,
        status_snapshot::{build_status_snapshot, StatusOptions},
    },
};

use crate::surface::mcp::{helpers::render_result, SynrepoState};

pub fn handle_overview(state: &SynrepoState) -> String {
    let result: anyhow::Result<serde_json::Value> = {
        let snapshot = build_status_snapshot(
            &state.repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        );
        let graph = snapshot.graph_stats.as_ref().map(|stats| {
            json!({
                "file_nodes": stats.file_nodes,
                "symbol_nodes": stats.symbol_nodes,
                "concept_nodes": stats.concept_nodes,
                "total_edges": stats.total_edges,
                "edges_by_kind": stats.edge_counts_by_kind,
            })
        });
        let matrix = snapshot.initialized.then(|| {
            let report = probe(&state.repo_root);
            let config = snapshot.config.clone().unwrap_or_default();
            ReadinessMatrix::build(&state.repo_root, &report, &snapshot, &config)
        });
        let readiness = matrix
            .as_ref()
            .map(|matrix| {
                matrix
                    .rows
                    .iter()
                    .map(|row| {
                        json!({
                            "capability": row.capability.as_str(),
                            "state": row.state.as_str(),
                            "detail": row.detail,
                            "next_action": row.next_action,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let diagnostics = snapshot.diagnostics.as_ref();
        let metrics = snapshot.context_metrics.as_ref();

        Ok(json!({
            "mode": state.config.mode.to_string(),
            "graph": graph.unwrap_or_else(|| json!({
                "file_nodes": 0,
                "symbol_nodes": 0,
                "concept_nodes": 0,
                "total_edges": 0,
                "edges_by_kind": {},
            })),
            "readiness": readiness,
            "watch": diagnostics.map(|diag| format!("{:?}", diag.watch_status)),
            "reconcile": diagnostics.map(|diag| {
                json!({
                    "health": format!("{:?}", diag.reconcile_health),
                    "last": diag.last_reconcile,
                })
            }),
            "writer": diagnostics.map(|diag| format!("{:?}", diag.writer_status)),
            "export_freshness": snapshot.export_freshness,
            "explain": snapshot.explain_provider.as_ref().map(|provider| {
                json!({
                    "provider": provider.provider,
                    "model": provider.model,
                    "local_endpoint": provider.local_endpoint,
                    "status": format!("{:?}", provider.status),
                })
            }),
            "commentary": {
                "coverage": snapshot.commentary_coverage.display,
                "total": snapshot.commentary_coverage.total,
                "fresh": snapshot.commentary_coverage.fresh,
            },
            "overlay_cost": snapshot.overlay_cost_summary,
            "agent_notes": snapshot.agent_note_counts,
            "metrics": metrics.map(|metrics| {
                json!({
                    "mcp_requests_total": metrics.mcp_requests_total,
                    "cards_served_total": metrics.cards_served_total,
                    "compact_outputs_total": metrics.compact_outputs_total,
                    "commentary_refresh_total": metrics.commentary_refresh_total,
                })
            }),
            "recent_activity": snapshot.recent_activity.unwrap_or_default(),
        }))
    };
    render_result(result)
}

pub fn handle_degraded_overview(repo_root: std::path::PathBuf, error: anyhow::Error) -> String {
    let result: anyhow::Result<serde_json::Value> = {
        let snapshot = build_status_snapshot(
            &repo_root,
            StatusOptions {
                recent: true,
                full: false,
            },
        );
        let report = probe(&repo_root);
        Ok(json!({
            "source_store": "degraded",
            "repo_root": repo_root,
            "initialized": snapshot.initialized,
            "error": crate::surface::mcp::error::error_value(&error)["error"].clone(),
            "graph": snapshot.graph_stats.map(|stats| json!({
                "file_nodes": stats.file_nodes,
                "symbol_nodes": stats.symbol_nodes,
                "concept_nodes": stats.concept_nodes,
                "total_edges": stats.total_edges,
                "edges_by_kind": stats.edge_counts_by_kind,
            })),
            "runtime_classification": format!("{:?}", report.classification),
            "watch": snapshot
                .diagnostics
                .as_ref()
                .map(|diag| format!("{:?}", diag.watch_status)),
            "reconcile": snapshot
                .diagnostics
                .as_ref()
                .map(|diag| format!("{:?}", diag.reconcile_health)),
            "recent_activity": snapshot.recent_activity.unwrap_or_default(),
        }))
    };
    render_result(result)
}
