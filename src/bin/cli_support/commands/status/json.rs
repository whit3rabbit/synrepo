//! JSON formatter for `synrepo status --json`.

use std::fmt::Write;

use synrepo::{
    pipeline::context_metrics::ContextMetrics,
    pipeline::diagnostics::{EmbeddingHealth, ReconcileHealth, WriterStatus},
    surface::readiness::ReadinessMatrix,
    surface::status_snapshot::{
        CommentaryCoverage, ExplainDisplay, GraphSnapshotStatus, RepairAuditState, StatusSnapshot,
    },
};

use super::helpers;

pub(super) fn write_status_json(
    out: &mut String,
    snapshot: &StatusSnapshot,
    readiness: Option<&ReadinessMatrix>,
) -> anyhow::Result<()> {
    if !snapshot.initialized {
        writeln!(out, "{{\"initialized\":false}}").unwrap();
        return Ok(());
    }
    let config = snapshot
        .config
        .as_ref()
        .expect("initialized implies config loaded");
    let diag = snapshot
        .diagnostics
        .as_ref()
        .expect("initialized implies diagnostics present");

    let graph_json = match &snapshot.graph_stats {
        Some(stats) => serde_json::json!({
            "file_nodes": stats.file_nodes,
            "symbol_nodes": stats.symbol_nodes,
            "concept_nodes": stats.concept_nodes,
        }),
        None => serde_json::Value::Null,
    };

    let (reconcile_health, reconcile_staleness_reason) = match &diag.reconcile_health {
        ReconcileHealth::Current => ("current", None),
        ReconcileHealth::Stale(synrepo::pipeline::diagnostics::ReconcileStaleness::Outcome(o)) => {
            ("stale", Some(format!("outcome: {o}")))
        }
        ReconcileHealth::Stale(synrepo::pipeline::diagnostics::ReconcileStaleness::Age {
            ..
        }) => ("stale", Some("age".to_string())),
        ReconcileHealth::WatchStalled { .. } => (
            "watch_stalled",
            Some("watch_running_old_reconcile".to_string()),
        ),
        ReconcileHealth::Unknown => ("unknown", None),
        ReconcileHealth::Corrupt(_) => ("corrupt", None),
    };

    let last_reconcile_at = diag
        .last_reconcile
        .as_ref()
        .map(|s| s.last_reconcile_at.as_str())
        .unwrap_or("");

    let writer_lock = match &diag.writer_status {
        WriterStatus::Free => "free".to_string(),
        WriterStatus::HeldBySelf => "held_by_self".to_string(),
        WriterStatus::HeldByOther { pid } => format!("held_by_pid_{pid}"),
        WriterStatus::Corrupt(_) => "corrupt".to_string(),
    };

    let watch = helpers::render_watch_summary(&diag.watch_status);

    let activity_json: serde_json::Value = match snapshot.recent_activity.as_deref() {
        Some(entries) => serde_json::to_value(entries).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    };

    let repair_audit_json = repair_audit_json(&snapshot.repair_audit);
    let commentary_json = commentary_json(&snapshot.commentary_coverage);

    let output = serde_json::json!({
        "initialized": true,
        "mode": config.mode.to_string(),
        "graph": graph_json,
        "reconcile_health": reconcile_health,
        "reconcile_staleness_reason": reconcile_staleness_reason,
        "last_reconcile_at": last_reconcile_at,
        "watch": watch,
        "writer_lock": writer_lock,
        "export_freshness": snapshot.export_freshness,
        "overlay_cost_summary": snapshot.overlay_cost_summary,
        "embedding_health": match &diag.embedding_health {
            EmbeddingHealth::Disabled => serde_json::json!({"status": "disabled"}),
            EmbeddingHealth::Available { model, dim, chunks } => serde_json::json!({
                "status": "available",
                "model": model,
                "dim": dim,
                "chunks": chunks,
            }),
            EmbeddingHealth::Degraded(reason) => serde_json::json!({
                "status": "degraded",
                "reason": reason,
            }),
        },
        "commentary_coverage": commentary_json,
        "agent_notes": snapshot.agent_note_counts,
        "graph_snapshot": graph_snapshot_json(&snapshot.graph_snapshot),
        "explain_provider": snapshot.explain_provider.as_ref().map(explain_json),
        "explain_totals": snapshot.explain_totals.as_ref().map(|t| serde_json::json!({
            "since": t.since,
            "updated_at": t.updated_at,
            "calls": t.calls,
            "input_tokens": t.input_tokens,
            "output_tokens": t.output_tokens,
            "failures": t.failures,
            "budget_blocked": t.budget_blocked,
            "usd_cost": t.usd_cost,
            "any_estimated": t.any_estimated,
            "any_unpriced": t.any_unpriced,
            "openrouter_live_pricing_used": super::openrouter_live_pricing_used(t),
            "pricing_basis": synrepo::pipeline::explain::pricing::pricing_basis_label(
                super::openrouter_live_pricing_used(t)
            ),
            "pricing_last_updated": synrepo::pipeline::explain::pricing::LAST_UPDATED,
            "per_provider": t.per_provider,
        })),
        "context_metrics": snapshot.context_metrics.as_ref().map(context_metrics_json),
        "recent_activity": activity_json,
        "last_compaction_timestamp": snapshot.last_compaction.map(|ts| ts.to_string()),
        "repair_audit": repair_audit_json,
        "capability_readiness": readiness.map(readiness_matrix_json),
    });

    writeln!(out, "{}", serde_json::to_string_pretty(&output)?).unwrap();
    Ok(())
}

fn context_metrics_json(metrics: &ContextMetrics) -> serde_json::Value {
    let mut value = serde_json::to_value(metrics).unwrap_or(serde_json::Value::Null);
    let Some(obj) = value.as_object_mut() else {
        return value;
    };
    obj.insert(
        "card_tokens_avg".to_string(),
        serde_json::json!(metrics.card_tokens_avg()),
    );
    obj.insert(
        "context_query_latency_ms_avg".to_string(),
        serde_json::json!(metrics.context_query_latency_ms_avg()),
    );
    value
}

fn commentary_json(coverage: &CommentaryCoverage) -> serde_json::Value {
    serde_json::json!({
        "total": coverage.total,
        "fresh": coverage.fresh,
        "estimated_fresh": coverage.estimated_fresh,
        "estimated_stale_ratio": coverage.estimated_stale_ratio,
        "estimate_confidence": coverage.estimate_confidence,
    })
}

fn graph_snapshot_json(snapshot: &GraphSnapshotStatus) -> serde_json::Value {
    serde_json::json!({
        "snapshot_epoch": snapshot.epoch,
        "snapshot_age_ms": snapshot.age_ms,
        "snapshot_size_bytes": snapshot.size_bytes,
        "files": snapshot.file_count,
        "symbols": snapshot.symbol_count,
        "edges": snapshot.edge_count,
    })
}

fn explain_json(display: &ExplainDisplay) -> serde_json::Value {
    use synrepo::pipeline::explain::ExplainStatus;

    let (status_str, detected_env_var) = match &display.status {
        ExplainStatus::Enabled => ("enabled", None),
        ExplainStatus::DisabledKeyDetected { env_var } => ("disabled_key_detected", Some(*env_var)),
        ExplainStatus::Disabled => ("disabled", None),
    };
    serde_json::json!({
        "provider": display.provider,
        "model": display.model,
        "local_endpoint": display.local_endpoint,
        "endpoint_source": display.endpoint_source.display_label(),
        "status": status_str,
        "detected_env_var": detected_env_var,
    })
}

fn readiness_matrix_json(matrix: &ReadinessMatrix) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = matrix
        .rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "capability": row.capability.as_str(),
                "state": row.state.as_str(),
                "severity": row.state.severity().as_str(),
                "detail": row.detail,
                "next_action": row.next_action,
            })
        })
        .collect();
    serde_json::Value::Array(rows)
}

fn repair_audit_json(state: &RepairAuditState) -> serde_json::Value {
    match state {
        RepairAuditState::Ok => serde_json::json!({ "status": "ok" }),
        RepairAuditState::Unavailable {
            last_failure_at,
            last_failure_reason,
        } => serde_json::json!({
            "status": "unavailable",
            "last_failure_at": last_failure_at,
            "last_failure_reason": last_failure_reason,
        }),
    }
}
