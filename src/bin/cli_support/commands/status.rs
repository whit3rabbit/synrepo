use std::fmt::Write;
use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        compact::load_last_compaction_timestamp,
        diagnostics::{collect_diagnostics, ReconcileHealth, RuntimeDiagnostics, WriterStatus},
        export::load_manifest,
        recent_activity::{read_recent_activity, ActivityEntry, RecentActivityQuery},
        watch::WatchServiceStatus,
    },
    store::sqlite::SqliteGraphStore,
};

/// Print operational health: mode, graph counts, reconcile status, and watch state.
pub(crate) fn status(repo_root: &Path, json: bool, recent: bool) -> anyhow::Result<()> {
    let rendered = status_output(repo_root, json, recent)?;
    print!("{rendered}");
    Ok(())
}

/// Render the status output as a String (test-friendly equivalent of `status`).
/// Output is identical to what `status` prints, including trailing newlines.
pub(crate) fn status_output(repo_root: &Path, json: bool, recent: bool) -> anyhow::Result<String> {
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

    let export_freshness = export_freshness_summary(repo_root, &synrepo_dir, &config);
    let overlay_cost = overlay_cost_summary(&synrepo_dir);
    let last_compaction = load_last_compaction_timestamp(&synrepo_dir);

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
            recent_entries.as_deref(),
            last_compaction.as_ref(),
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
        render_watch_summary(&diag.watch_status)
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
        commentary_coverage_line(&synrepo_dir)
    )
    .unwrap();
    writeln!(out, "  export:       {export_freshness}").unwrap();
    writeln!(out, "  overlay cost: {overlay_cost}").unwrap();
    if let Some(ts) = last_compaction {
        writeln!(out, "  last compact:  {}", ts).unwrap();
    } else {
        writeln!(out, "  last compact:  never").unwrap();
    }
    writeln!(
        out,
        "  next step:    {}",
        next_step(&diag, graph_stats.is_none())
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
    recent_activity: Option<&[ActivityEntry]>,
    last_compaction: Option<&time::OffsetDateTime>,
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

    let watch = render_watch_summary(&diag.watch_status);

    let activity_json: serde_json::Value = match recent_activity {
        Some(entries) => serde_json::to_value(entries).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
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
        "recent_activity": activity_json,
        "last_compaction_timestamp": last_compaction.map(|ts| ts.to_string()),
    });

    writeln!(out, "{}", serde_json::to_string_pretty(&output)?).unwrap();
    Ok(())
}

/// Describe export freshness for status output.
fn export_freshness_summary(repo_root: &Path, synrepo_dir: &Path, config: &Config) -> String {
    use synrepo::pipeline::watch::load_reconcile_state;

    let manifest = load_manifest(repo_root, config);
    match manifest {
        None => "absent (run `synrepo export` to generate)".to_string(),
        Some(m) => {
            let current_epoch = load_reconcile_state(synrepo_dir)
                .map(|r| r.last_reconcile_at)
                .unwrap_or_default();
            if m.last_reconcile_at == current_epoch {
                format!("current ({}, {})", m.format.as_str(), m.budget)
            } else {
                format!(
                    "stale (generated at {}, current epoch {})",
                    m.last_reconcile_at, current_epoch
                )
            }
        }
    }
}

/// Describe overlay cost for status output. Scans on demand; no caching.
fn overlay_cost_summary(synrepo_dir: &Path) -> String {
    use synrepo::store::overlay::SqliteOverlayStore;

    let overlay_dir = synrepo_dir.join("overlay");
    let db = SqliteOverlayStore::db_path(&overlay_dir);
    if !db.exists() {
        return "no overlay (0 LLM calls)".to_string();
    }

    let overlay = match SqliteOverlayStore::open_existing(&overlay_dir) {
        Ok(o) => o,
        Err(e) => return format!("unavailable ({e})"),
    };

    // Surface query failures as "unavailable (...)" rather than collapsing to
    // zero. A silent zero count is indistinguishable from a healthy overlay
    // with no LLM activity, which masks operational issues.
    let cross_link_gens = match overlay.cross_link_generation_count() {
        Ok(n) => n,
        Err(e) => return format!("unavailable (cross-link count query failed: {e})"),
    };
    let commentary_entries = match overlay.commentary_count() {
        Ok(n) => n,
        Err(e) => return format!("unavailable (commentary count query failed: {e})"),
    };
    let total_calls = cross_link_gens + commentary_entries;
    let pending_promotion = match overlay.cross_link_state_counts() {
        Ok(counts) => counts.pending_promotion,
        Err(e) => return format!("unavailable (cross-link state count query failed: {e})"),
    };

    format!(
        "{total_calls} LLM calls ({cross_link_gens} cross-link gen, {commentary_entries} commentary){pending_promotion_str}",
        pending_promotion_str = if pending_promotion > 0 {
            format!(", {pending_promotion} pending promotion")
        } else {
            String::new()
        }
    )
}

pub(super) fn render_watch_summary(status: &WatchServiceStatus) -> String {
    match status {
        WatchServiceStatus::Inactive => "inactive".to_string(),
        WatchServiceStatus::Running(state) => {
            format!("{} mode (pid {})", state.mode, state.pid)
        }
        WatchServiceStatus::Stale(Some(state)) => {
            format!("stale lease from pid {}", state.pid)
        }
        WatchServiceStatus::Stale(None) => "stale watch artifacts".to_string(),
        WatchServiceStatus::Corrupt(e) => format!("corrupt ({e})"),
    }
}

fn commentary_coverage_line(synrepo_dir: &Path) -> String {
    use std::str::FromStr;
    use synrepo::core::ids::NodeId;
    use synrepo::pipeline::repair::resolve_commentary_node;
    use synrepo::store::overlay::SqliteOverlayStore;
    use synrepo::store::sqlite::SqliteGraphStore;

    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_db = SqliteOverlayStore::db_path(&overlay_dir);
    if !overlay_db.exists() {
        return "not initialized".to_string();
    }

    let overlay = match SqliteOverlayStore::open_existing(&overlay_dir) {
        Ok(store) => store,
        Err(error) => return format!("unavailable ({error})"),
    };
    let rows = match overlay.commentary_hashes() {
        Ok(rows) => rows,
        Err(error) => return format!("unavailable ({error})"),
    };
    if rows.is_empty() {
        return "0 entries".to_string();
    }

    let graph = match SqliteGraphStore::open_existing(&synrepo_dir.join("graph")) {
        Ok(graph) => graph,
        Err(_) => return format!("{} entries (graph unreadable)", rows.len()),
    };

    let fresh = synrepo::structure::graph::with_graph_read_snapshot(&graph, |graph| {
        let mut fresh = 0usize;
        for (node_id_str, stored_hash) in &rows {
            let Ok(node_id) = NodeId::from_str(node_id_str) else {
                continue;
            };
            if resolve_commentary_node(graph, node_id)
                .ok()
                .flatten()
                .is_some_and(|snap| &snap.content_hash == stored_hash)
            {
                fresh += 1;
            }
        }
        Ok(fresh)
    })
    .unwrap_or(0);

    format!("{fresh} fresh / {} total nodes with commentary", rows.len())
}

fn next_step(diag: &RuntimeDiagnostics, graph_missing: bool) -> &'static str {
    if graph_missing {
        return "run `synrepo init` to materialize the graph";
    }
    match (
        &diag.reconcile_health,
        &diag.writer_status,
        &diag.watch_status,
    ) {
        (_, _, WatchServiceStatus::Running(_)) => {
            "watch service is active — use `synrepo watch status` for runtime details"
        }
        (ReconcileHealth::Corrupt(_), _, _) => {
            "reconcile state is corrupt — run `synrepo watch stop` to clean up and recover"
        }
        (_, WriterStatus::Corrupt(_), _) => {
            "writer lock is corrupt — remove .synrepo/state/writer.lock to recover"
        }
        (_, _, WatchServiceStatus::Corrupt(_)) => {
            "watch state is corrupt — run `synrepo watch stop` to clean up and recover"
        }
        (_, WriterStatus::HeldByOther { .. }, _) => {
            "writer lock is held — wait for the other process or verify it is still alive"
        }
        (ReconcileHealth::Unknown, _, _) => "run `synrepo reconcile` to do the first graph pass",
        (ReconcileHealth::Stale(_), _, _) => "run `synrepo reconcile` to refresh the graph",
        (ReconcileHealth::Current, _, _) => {
            "graph is current — use `synrepo graph query` or connect the MCP server"
        }
    }
}
