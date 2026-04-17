//! Status command implementation.
//! @modified Refactored from 628-line single file into status/ submodule directory

mod export;
mod helpers;
mod overlay;

pub(crate) use helpers::render_watch_summary;

use std::fmt::Write;
use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        compact::load_last_compaction_timestamp,
        diagnostics::{
            collect_diagnostics, EmbeddingHealth, ReconcileHealth, RuntimeDiagnostics, WriterStatus,
        },
        recent_activity::{read_recent_activity, ActivityEntry, RecentActivityQuery},
    },
    store::sqlite::SqliteGraphStore,
};

/// Print operational health: mode, graph counts, reconcile status, and watch state.
pub(crate) fn status(repo_root: &Path, json: bool, recent: bool, full: bool) -> anyhow::Result<()> {
    let rendered = status_output(repo_root, json, recent, full)?;
    print!("{rendered}");
    Ok(())
}

/// Render the status output as a String (test-friendly equivalent of `status`).
/// Output is identical to what `status` prints, including trailing newlines.
///
/// `full` enables the commentary freshness scan (O(commentary_rows × graph_lookup)).
/// Default status leaves it off so the command stays cheap enough to run habitually.
pub(crate) fn status_output(
    repo_root: &Path,
    json: bool,
    recent: bool,
    full: bool,
) -> anyhow::Result<String> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let mut out = String::new();

    let config = match Config::load(repo_root) {
        Ok(config) => config,
        Err(_) => {
            if json {
                writeln!(out, "{{\"initialized\":false}}").unwrap();
            } else {
                writeln!(out, "synrepo status: not initialized").unwrap();
                writeln!(
                    out,
                    "  Run `synrepo init` to create .synrepo/ and populate the graph."
                )
                .unwrap();
            }
            return Ok(out);
        }
    };

    let diag = collect_diagnostics(&synrepo_dir, &config);
    let graph_stats = {
        let graph_dir = synrepo_dir.join("graph");
        SqliteGraphStore::open_existing(&graph_dir)
            .ok()
            .and_then(|store| {
                synrepo::structure::graph::with_graph_read_snapshot(&store, |_graph| {
                    store.persisted_stats()
                })
                .ok()
            })
    };

    let export_freshness = export::export_freshness_summary(repo_root, &synrepo_dir, &config);
    let overlay = overlay::open_status_overlay(&synrepo_dir);
    let overlay_cost = overlay::overlay_cost_summary(&overlay);
    let commentary = overlay::commentary_coverage(&synrepo_dir, full, &overlay);
    let last_compaction = load_last_compaction_timestamp(&synrepo_dir);
    let repair_audit = helpers::load_repair_audit_state(&synrepo_dir);

    let recent_entries: Option<Vec<ActivityEntry>> = if recent {
        let query = RecentActivityQuery {
            kinds: None,
            limit: 20,
            since: None,
        };
        read_recent_activity(&synrepo_dir, repo_root, &config, query).ok()
    } else {
        None
    };

    if json {
        write_status_json(
            &mut out,
            &config,
            &diag,
            graph_stats.as_ref(),
            &export_freshness,
            &overlay_cost,
            &commentary,
            recent_entries.as_deref(),
            last_compaction.as_ref(),
            &repair_audit,
        )?;
        return Ok(out);
    }

    writeln!(out, "synrepo status").unwrap();
    writeln!(out, "  mode:         {}", config.mode).unwrap();

    match &graph_stats {
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

    writeln!(out, "  commentary:   {}", commentary.display).unwrap();
    writeln!(out, "  export:       {export_freshness}").unwrap();
    writeln!(out, "  overlay cost: {overlay_cost}").unwrap();
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
    if let Some(ts) = last_compaction {
        writeln!(out, "  last compact:  {}", ts).unwrap();
    } else {
        writeln!(out, "  last compact:  never").unwrap();
    }
    writeln!(
        out,
        "  repair audit: {}",
        helpers::render_repair_audit(&repair_audit)
    )
    .unwrap();
    writeln!(
        out,
        "  next step:    {}",
        helpers::next_step(&diag, graph_stats.is_none())
    )
    .unwrap();

    if let Some(entries) = &recent_entries {
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
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn write_status_json(
    out: &mut String,
    config: &Config,
    diag: &RuntimeDiagnostics,
    graph_stats: Option<&synrepo::store::sqlite::PersistedGraphStats>,
    export_freshness: &str,
    overlay_cost: &str,
    commentary: &overlay::CommentaryCoverage,
    recent_activity: Option<&[ActivityEntry]>,
    last_compaction: Option<&time::OffsetDateTime>,
    repair_audit: &helpers::RepairAuditState,
) -> anyhow::Result<()> {
    let graph_json = match graph_stats {
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

    let activity_json: serde_json::Value = match recent_activity {
        Some(entries) => serde_json::to_value(entries).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    };

    let repair_audit_json = match repair_audit {
        helpers::RepairAuditState::Ok => serde_json::json!({ "status": "ok" }),
        helpers::RepairAuditState::Unavailable {
            last_failure_at,
            last_failure_reason,
        } => {
            serde_json::json!({
                "status": "unavailable",
                "last_failure_at": last_failure_at,
                "last_failure_reason": last_failure_reason,
            })
        }
    };

    let output = serde_json::json!({
        "initialized": true,
        "mode": config.mode.to_string(),
        "graph": graph_json,
        "reconcile_health": reconcile_health,
        "reconcile_staleness_reason": reconcile_staleness_reason,
        "last_reconcile_at": last_reconcile_at,
        "watch": watch,
        "writer_lock": writer_lock,
        "export_freshness": export_freshness,
        "overlay_cost_summary": overlay_cost,
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
        "commentary_coverage": {
            "total": commentary.total,
            "fresh": commentary.fresh,
        },
        "recent_activity": activity_json,
        "last_compaction_timestamp": last_compaction.map(|ts| ts.to_string()),
        "repair_audit": repair_audit_json,
    });

    writeln!(out, "{}", serde_json::to_string_pretty(&output)?).unwrap();
    Ok(())
}
