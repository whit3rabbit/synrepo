//! Status command implementation.
//!
//! Pure formatter over `synrepo::surface::status_snapshot::StatusSnapshot`.

mod helpers;

pub(crate) use helpers::render_watch_summary;

use std::fmt::Write;
use std::path::Path;

use synrepo::{
    pipeline::{
        diagnostics::{EmbeddingHealth, ReconcileHealth, WriterStatus},
        explain::ExplainStatus,
    },
    surface::status_snapshot::{
        build_status_snapshot, CommentaryCoverage, ExplainDisplay, GraphSnapshotStatus,
        RepairAuditState, StatusOptions, StatusSnapshot,
    },
};

/// Print operational health: mode, graph counts, reconcile status, and watch state.
pub(crate) fn status(repo_root: &Path, json: bool, recent: bool, full: bool) -> anyhow::Result<()> {
    let rendered = status_output(repo_root, json, recent, full)?;
    print!("{rendered}");
    Ok(())
}

/// Render the status output as a String. Used by `cli.rs` for the non-TTY
/// fallback under bare `synrepo` on a ready repo, and by tests.
pub(crate) fn status_output(
    repo_root: &Path,
    json: bool,
    recent: bool,
    full: bool,
) -> anyhow::Result<String> {
    let snapshot = build_status_snapshot(repo_root, StatusOptions { recent, full });
    let mut out = String::new();
    if json {
        write_status_json(&mut out, &snapshot)?;
    } else {
        write_status_text(&mut out, &snapshot, full);
    }
    Ok(out)
}

fn write_status_text(out: &mut String, snapshot: &StatusSnapshot, full: bool) {
    if !snapshot.initialized {
        writeln!(out, "synrepo status: not initialized").unwrap();
        writeln!(
            out,
            "  Run `synrepo init` to create .synrepo/ and populate the graph."
        )
        .unwrap();
        return;
    }
    let config = snapshot
        .config
        .as_ref()
        .expect("initialized implies config loaded");
    let diag = snapshot
        .diagnostics
        .as_ref()
        .expect("initialized implies diagnostics present");

    writeln!(out, "synrepo status").unwrap();
    writeln!(out, "  mode:         {}", config.mode).unwrap();

    match &snapshot.graph_stats {
        Some(stats) => writeln!(
            out,
            "  graph:        {} files  {} symbols  {} concepts",
            stats.file_nodes, stats.symbol_nodes, stats.concept_nodes
        )
        .unwrap(),
        None => writeln!(out, "  graph:        not materialized — run `synrepo init`").unwrap(),
    }

    match &diag.reconcile_health {
        ReconcileHealth::Current => writeln!(out, "  reconcile:    current").unwrap(),
        ReconcileHealth::Stale(synrepo::pipeline::diagnostics::ReconcileStaleness::Outcome(o)) => {
            writeln!(out, "  reconcile:    stale (last outcome: {o})").unwrap()
        }
        ReconcileHealth::Stale(synrepo::pipeline::diagnostics::ReconcileStaleness::Age {
            ..
        }) => writeln!(out, "  reconcile:    stale (over 1 hour old)").unwrap(),
        ReconcileHealth::Unknown => writeln!(out, "  reconcile:    unknown (never run)").unwrap(),
        ReconcileHealth::Corrupt(e) => writeln!(out, "  reconcile:    corrupt ({e})").unwrap(),
    }

    if let Some(state) = &diag.last_reconcile {
        let detail = match (state.files_discovered, state.symbols_extracted) {
            (Some(files), Some(symbols)) => {
                format!(
                    "completed — {files} files, {symbols} symbols ({} events)",
                    state.triggering_events
                )
            }
            _ => format!(
                "{} ({} events)",
                state.last_outcome, state.triggering_events
            ),
        };
        writeln!(
            out,
            "  last run:     {} — {detail}",
            state.last_reconcile_at
        )
        .unwrap();
        if let Some(error) = &state.last_error {
            writeln!(out, "  error:        {error}").unwrap();
        }
    }

    writeln!(
        out,
        "  watch:        {}",
        helpers::render_watch_summary(&diag.watch_status)
    )
    .unwrap();

    match &diag.writer_status {
        WriterStatus::Free => writeln!(out, "  writer lock:  free").unwrap(),
        WriterStatus::HeldBySelf => writeln!(out, "  writer lock:  held by this process").unwrap(),
        WriterStatus::HeldByOther { pid } => {
            writeln!(out, "  writer lock:  held by pid {pid}").unwrap()
        }
        WriterStatus::Corrupt(e) => writeln!(out, "  writer lock:  corrupt ({e})").unwrap(),
    }

    for line in &diag.store_guidance {
        writeln!(out, "  store:        {line}").unwrap();
    }

    writeln!(
        out,
        "  commentary:   {}",
        snapshot.commentary_coverage.display
    )
    .unwrap();
    writeln!(out, "  export:       {}", snapshot.export_freshness).unwrap();
    writeln!(out, "  overlay cost: {}", snapshot.overlay_cost_summary).unwrap();
    if full {
        writeln!(
            out,
            "  snapshot:     epoch {}  age {}  size {} ({} files / {} symbols / {} edges)",
            snapshot.graph_snapshot.epoch,
            render_snapshot_age(snapshot.graph_snapshot.age_ms),
            render_snapshot_size(snapshot.graph_snapshot.size_bytes),
            snapshot.graph_snapshot.file_count,
            snapshot.graph_snapshot.symbol_count,
            snapshot.graph_snapshot.edge_count,
        )
        .unwrap();
    }
    match &diag.embedding_health {
        EmbeddingHealth::Disabled => {}
        EmbeddingHealth::Available { model, dim, chunks } => {
            writeln!(
                out,
                "  embedding:    available ({model}, {dim}d, {chunks} chunks)"
            )
            .unwrap();
        }
        EmbeddingHealth::Degraded(reason) => {
            writeln!(out, "  embedding:    degraded ({reason})").unwrap();
        }
    }
    if let Some(ts) = snapshot.last_compaction {
        writeln!(out, "  last compact:  {}", ts).unwrap();
    } else {
        writeln!(out, "  last compact:  never").unwrap();
    }
    writeln!(
        out,
        "  repair audit: {}",
        helpers::render_repair_audit(&snapshot.repair_audit)
    )
    .unwrap();

    // Explain provider status
    if let Some(explain) = &snapshot.explain_provider {
        writeln!(out, "  explain:    {}", render_explain_line(explain)).unwrap();
    } else {
        writeln!(out, "  explain:    not initialized").unwrap();
    }
    if let Some(totals) = &snapshot.explain_totals {
        let total_calls = totals.calls + totals.failures + totals.budget_blocked;
        if total_calls > 0 {
            let openrouter_live = openrouter_live_pricing_used(totals);
            let est = if totals.any_estimated { " (est.)" } else { "" };
            let cost = if totals.any_unpriced {
                format!("${:.4} + unpriced", totals.usd_cost)
            } else {
                format!("${:.4}", totals.usd_cost)
            };
            writeln!(
                out,
                "    usage:      {} call(s), {} in / {} out tokens{}, {} (pricing as of {})",
                total_calls,
                totals.input_tokens,
                totals.output_tokens,
                est,
                cost,
                synrepo::pipeline::explain::pricing::pricing_basis_label(openrouter_live)
            )
            .unwrap();
            if totals.failures > 0 || totals.budget_blocked > 0 {
                writeln!(
                    out,
                    "    skipped:    {} failed, {} budget-blocked",
                    totals.failures, totals.budget_blocked
                )
                .unwrap();
            }
        }
    }

    if let Some(metrics) = &snapshot.context_metrics {
        if metrics.cards_served_total > 0 {
            writeln!(
                out,
                "  context:    {} card(s), {:.1} avg tokens/card, {} est. tokens avoided",
                metrics.cards_served_total,
                metrics.card_tokens_avg(),
                metrics.estimated_tokens_saved_total
            )
            .unwrap();
        }
    }

    writeln!(
        out,
        "  next step:    {}",
        helpers::next_step(diag, snapshot.graph_stats.is_none())
    )
    .unwrap();

    if let Some(entries) = &snapshot.recent_activity {
        writeln!(out).unwrap();
        writeln!(out, "recent activity:").unwrap();
        if entries.is_empty() {
            writeln!(out, "  (none)").unwrap();
        }
        for entry in entries {
            if entry.timestamp.is_empty() {
                writeln!(out, "  [{}] {}", entry.kind, entry.payload).unwrap();
            } else {
                writeln!(
                    out,
                    "  {} [{}] {}",
                    entry.timestamp, entry.kind, entry.payload
                )
                .unwrap();
            }
        }
    }
}

fn write_status_json(out: &mut String, snapshot: &StatusSnapshot) -> anyhow::Result<()> {
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
            "openrouter_live_pricing_used": openrouter_live_pricing_used(t),
            "pricing_basis": synrepo::pipeline::explain::pricing::pricing_basis_label(
                openrouter_live_pricing_used(t)
            ),
            "pricing_last_updated": synrepo::pipeline::explain::pricing::LAST_UPDATED,
            "per_provider": t.per_provider,
        })),
        "context_metrics": snapshot.context_metrics.as_ref().map(|m| serde_json::json!({
            "cards_served_total": m.cards_served_total,
            "card_tokens_total": m.card_tokens_total,
            "card_tokens_avg": m.card_tokens_avg(),
            "raw_file_tokens_total": m.raw_file_tokens_total,
            "estimated_tokens_saved_total": m.estimated_tokens_saved_total,
            "budget_tier_usage": m.budget_tier_usage,
            "truncation_applied_total": m.truncation_applied_total,
            "stale_responses_total": m.stale_responses_total,
            "test_surface_hits_total": m.test_surface_hits_total,
            "changed_files_total": m.changed_files_total,
            "context_query_latency_ms_avg": m.context_query_latency_ms_avg(),
        })),
        "recent_activity": activity_json,
        "last_compaction_timestamp": snapshot.last_compaction.map(|ts| ts.to_string()),
        "repair_audit": repair_audit_json,
    });

    writeln!(out, "{}", serde_json::to_string_pretty(&output)?).unwrap();
    Ok(())
}

fn openrouter_live_pricing_used(
    totals: &synrepo::pipeline::explain::accounting::ExplainTotals,
) -> bool {
    totals
        .per_provider
        .get("openrouter")
        .and_then(|provider| provider.usd_cost)
        .is_some()
}

fn commentary_json(coverage: &CommentaryCoverage) -> serde_json::Value {
    serde_json::json!({
        "total": coverage.total,
        "fresh": coverage.fresh,
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

fn render_snapshot_age(age_ms: u64) -> String {
    if age_ms < 1_000 {
        format!("{age_ms}ms")
    } else {
        format!("{:.1}s", age_ms as f64 / 1_000.0)
    }
}

fn render_snapshot_size(size_bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if size_bytes >= 1024 * 1024 {
        format!("{:.1} MiB", size_bytes as f64 / MIB)
    } else if size_bytes >= 1024 {
        format!("{:.1} KiB", size_bytes as f64 / KIB)
    } else {
        format!("{size_bytes} B")
    }
}

fn render_explain_line(display: &ExplainDisplay) -> String {
    let provider_and_model = match &display.model {
        Some(m) => format!("{} ({})", display.provider, m),
        None => display.provider.clone(),
    };
    match &display.status {
        ExplainStatus::Enabled => {
            let mut line = provider_and_model;
            if let Some(endpoint) = &display.local_endpoint {
                let source = display.endpoint_source.display_label();
                write!(line, " @ {endpoint} [source: {source}]").unwrap();
            }
            line
        }
        ExplainStatus::DisabledKeyDetected { env_var } => {
            format!(
                "disabled ({env_var} detected; run 'synrepo setup <tool> --explain' \
                 to enable, or set [explain] enabled = true in .synrepo/config.toml \
                 and store reusable keys or local endpoints in ~/.synrepo/config.toml)"
            )
        }
        ExplainStatus::Disabled => "disabled".to_string(),
    }
}

fn explain_json(display: &ExplainDisplay) -> serde_json::Value {
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
