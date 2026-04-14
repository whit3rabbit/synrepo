use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::{
        diagnostics::{collect_diagnostics, ReconcileHealth, RuntimeDiagnostics, WriterStatus},
        export::load_manifest,
        recent_activity::{read_recent_activity, ActivityEntry, RecentActivityQuery},
        watch::WatchServiceStatus,
    },
    store::sqlite::SqliteGraphStore,
};

/// Print operational health: mode, graph counts, reconcile status, and watch state.
pub(crate) fn status(repo_root: &Path, json: bool, recent: bool) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);

    let config = match Config::load(repo_root) {
        Ok(config) => config,
        Err(_) => {
            if json {
                println!("{{\"initialized\":false}}");
            } else {
                println!("synrepo status: not initialized");
                println!("  Run `synrepo init` to create .synrepo/ and populate the graph.");
            }
            return Ok(());
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
        return print_status_json(
            &config,
            &diag,
            graph_stats.as_ref(),
            &export_freshness,
            &overlay_cost,
            recent_entries.as_deref(),
        );
    }

    println!("synrepo status");
    println!("  mode:         {}", config.mode);

    match &graph_stats {
        Some(stats) => println!(
            "  graph:        {} files  {} symbols  {} concepts",
            stats.file_nodes, stats.symbol_nodes, stats.concept_nodes
        ),
        None => println!("  graph:        not materialized — run `synrepo init`"),
    }

    match &diag.reconcile_health {
        ReconcileHealth::Current => println!("  reconcile:    current"),
        ReconcileHealth::Stale { last_outcome } => {
            println!("  reconcile:    stale (last outcome: {last_outcome})")
        }
        ReconcileHealth::Unknown => println!("  reconcile:    unknown (never run)"),
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
        println!("  last run:     {} — {detail}", state.last_reconcile_at);
        if let Some(error) = &state.last_error {
            println!("  error:        {error}");
        }
    }

    println!(
        "  watch:        {}",
        render_watch_summary(&diag.watch_status)
    );

    match &diag.writer_status {
        WriterStatus::Free => println!("  writer lock:  free"),
        WriterStatus::HeldBySelf => println!("  writer lock:  held by this process"),
        WriterStatus::HeldByOther { pid } => println!("  writer lock:  held by pid {pid}"),
    }

    for line in &diag.store_guidance {
        println!("  store:        {line}");
    }

    println!("  commentary:   {}", commentary_coverage_line(&synrepo_dir));
    println!("  export:       {export_freshness}");
    println!("  overlay cost: {overlay_cost}");
    println!(
        "  next step:    {}",
        next_step(&diag, graph_stats.is_none())
    );

    if let Some(entries) = &recent_entries {
        println!();
        println!("recent activity:");
        if entries.is_empty() {
            println!("  (none)");
        }
        for entry in entries {
            if entry.timestamp.is_empty() {
                println!("  [{}] {}", entry.kind, entry.payload);
            } else {
                println!("  {} [{}] {}", entry.timestamp, entry.kind, entry.payload);
            }
        }
    }
    Ok(())
}

fn print_status_json(
    config: &Config,
    diag: &RuntimeDiagnostics,
    graph_stats: Option<&synrepo::store::sqlite::PersistedGraphStats>,
    export_freshness: &str,
    overlay_cost: &str,
    recent_activity: Option<&[ActivityEntry]>,
) -> anyhow::Result<()> {
    let graph_json = match graph_stats {
        Some(stats) => serde_json::json!({
            "file_nodes": stats.file_nodes,
            "symbol_nodes": stats.symbol_nodes,
            "concept_nodes": stats.concept_nodes,
        }),
        None => serde_json::Value::Null,
    };

    let reconcile_health = match &diag.reconcile_health {
        ReconcileHealth::Current => "current",
        ReconcileHealth::Stale { .. } => "stale",
        ReconcileHealth::Unknown => "unknown",
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
        "last_reconcile_at": last_reconcile_at,
        "watch": watch,
        "writer_lock": writer_lock,
        "export_freshness": export_freshness,
        "overlay_cost_summary": overlay_cost,
        "recent_activity": activity_json,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
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

    let cross_link_gens = overlay.cross_link_generation_count().unwrap_or(0);
    let commentary_entries = overlay.commentary_count().unwrap_or(0);
    let total_calls = cross_link_gens + commentary_entries;

    format!(
        "{total_calls} LLM calls ({cross_link_gens} cross-link gen, {commentary_entries} commentary)"
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
        (_, WriterStatus::HeldByOther { .. }, _) => {
            "writer lock is held — wait for the other process or verify it is still alive"
        }
        (ReconcileHealth::Unknown, _, _) => "run `synrepo reconcile` to do the first graph pass",
        (ReconcileHealth::Stale { .. }, _, _) => "run `synrepo reconcile` to refresh the graph",
        (ReconcileHealth::Current, _, _) => {
            "graph is current — use `synrepo graph query` or connect the MCP server"
        }
    }
}
