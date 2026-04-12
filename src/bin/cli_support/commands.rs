use std::path::Path;

use synrepo::{
    config::{Config, Mode},
    pipeline::{
        diagnostics::{collect_diagnostics, ReconcileHealth, RuntimeDiagnostics, WriterStatus},
        repair::{build_repair_report, execute_sync},
        watch::{persist_reconcile_state, run_reconcile_pass, ReconcileOutcome},
    },
    store::{compatibility::StoreId, sqlite::SqliteGraphStore},
};

use super::{
    agent_shims::AgentTool,
    graph::{check_store_ready, graph_query_output, graph_stats_output, node_output},
};

/// Initialize the repository with the specified mode.
pub(crate) fn init(repo_root: &Path, requested_mode: Option<Mode>) -> anyhow::Result<()> {
    let report = synrepo::bootstrap::bootstrap(repo_root, requested_mode)?;
    print!("{}", report.render());
    Ok(())
}

/// Print operational health: mode, graph node counts, last reconcile outcome, lock state.
///
/// Read-only. Never acquires the writer lock or mutates any store. Safe to call
/// at any time, including while a reconcile is in progress.
pub(crate) fn status(repo_root: &Path) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);

    let config = match Config::load(repo_root) {
        Ok(c) => c,
        Err(_) => {
            println!("synrepo status: not initialized");
            println!("  Run `synrepo init` to create .synrepo/ and populate the graph.");
            return Ok(());
        }
    };

    let diag = collect_diagnostics(&synrepo_dir, &config);

    let graph_stats = {
        let graph_dir = synrepo_dir.join("graph");
        SqliteGraphStore::open_existing(&graph_dir)
            .ok()
            .and_then(|store| store.persisted_stats().ok())
    };

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
            (Some(f), Some(s)) => {
                format!(
                    "completed — {f} files, {s} symbols ({} events)",
                    state.triggering_events
                )
            }
            _ => format!(
                "{} ({} events)",
                state.last_outcome, state.triggering_events
            ),
        };
        println!("  last run:     {} — {detail}", state.last_reconcile_at);
        if let Some(err) = &state.last_error {
            println!("  error:        {err}");
        }
    }

    match &diag.writer_status {
        WriterStatus::Free => println!("  writer lock:  free"),
        WriterStatus::HeldBySelf => println!("  writer lock:  held by this process"),
        WriterStatus::HeldByOther { pid } => println!("  writer lock:  held by pid {pid}"),
    }

    for line in &diag.store_guidance {
        println!("  store:        {line}");
    }

    println!(
        "  next step:    {}",
        next_step(&diag, graph_stats.is_none())
    );
    Ok(())
}

/// Generate a thin integration shim for the specified agent CLI.
///
/// Writes a named fragment file and prints the one-line include instruction.
/// Never modifies existing user configuration. Pass `force = true` to overwrite.
pub(crate) fn agent_setup(repo_root: &Path, tool: AgentTool, force: bool) -> anyhow::Result<()> {
    let out_path = tool.output_path(repo_root);

    if out_path.exists() && !force {
        println!(
            "synrepo agent-setup: {} already exists.",
            out_path.display()
        );
        println!("  Pass --force to overwrite.");
        return Ok(());
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("could not create {}: {e}", parent.display()))?;
    }

    std::fs::write(&out_path, tool.shim_content())
        .map_err(|e| anyhow::anyhow!("could not write {}: {e}", out_path.display()))?;

    println!("Wrote {} shim: {}", tool.display_name(), out_path.display());
    println!("  {}", tool.include_instruction());
    Ok(())
}

fn next_step(diag: &RuntimeDiagnostics, graph_missing: bool) -> &'static str {
    if graph_missing {
        return "run `synrepo init` to materialize the graph";
    }
    match (&diag.reconcile_health, &diag.writer_status) {
        (_, WriterStatus::HeldByOther { .. }) => {
            "writer lock is held — wait for the other process or verify it is still alive"
        }
        (ReconcileHealth::Unknown, _) => "run `synrepo reconcile` to do the first graph pass",
        (ReconcileHealth::Stale { .. }, _) => "run `synrepo reconcile` to refresh the graph",
        (ReconcileHealth::Current, _) => {
            "graph is current — use `synrepo graph query` or connect the MCP server (phase 2)"
        }
    }
}

/// Report drift across all repair surfaces. Read-only; never mutates state.
///
/// Exits non-zero when any actionable or blocked findings are present so CI
/// can detect drift without running a repair.
pub(crate) fn check(repo_root: &Path, json_output: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root)
        .map_err(|e| anyhow::anyhow!("check: not initialized — run `synrepo init` first ({e})"))?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let report = build_repair_report(&synrepo_dir, &config);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.render());
    }

    if report.has_blocked() {
        return Err(anyhow::anyhow!(
            "check: blocked findings require manual intervention"
        ));
    }
    if report.has_actionable() {
        return Err(anyhow::anyhow!(
            "check: actionable findings detected — run `synrepo sync` to repair"
        ));
    }
    Ok(())
}

/// Repair auto-fixable drift surfaces and record the outcome.
///
/// Routes storage repairs through the maintenance plan and structural
/// refreshes through `run_reconcile_pass`. Report-only and unsupported
/// findings are surfaced but not touched. Appends a resolution log entry.
pub(crate) fn sync(repo_root: &Path, json_output: bool) -> anyhow::Result<()> {
    let config = Config::load(repo_root)
        .map_err(|e| anyhow::anyhow!("sync: not initialized — run `synrepo init` first ({e})"))?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    let summary = execute_sync(repo_root, &synrepo_dir, &config)?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print!("{}", summary.render());
    }

    if !summary.blocked.is_empty() {
        return Err(anyhow::anyhow!(
            "sync: {} finding(s) could not be repaired (blocked)",
            summary.blocked.len()
        ));
    }
    Ok(())
}

/// Reconcile the structural graph and update the index.
pub(crate) fn reconcile(repo_root: &Path) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    // No check_store_ready here: run_reconcile_pass handles the full compat
    // range. Blocking compat (schema mismatch) surfaces as ReconcileOutcome::Failed;
    // advisory compat (config drift, Rebuild) is corrected by the compile itself.

    let outcome = run_reconcile_pass(repo_root, &config, &synrepo_dir);
    persist_reconcile_state(&synrepo_dir, &outcome, 0);

    match &outcome {
        ReconcileOutcome::Completed(summary) => {
            println!(
                "Reconcile outcome: completed\n  files discovered: {}\n  symbols extracted: {}\n  concept nodes: {}",
                summary.files_discovered, summary.symbols_extracted, summary.concept_nodes_emitted,
            );
            Ok(())
        }
        ReconcileOutcome::LockConflict { holder_pid } => Err(anyhow::anyhow!(
            "Reconcile skipped: writer lock held by pid {holder_pid}. \
             Wait for that process to finish, then retry."
        )),
        ReconcileOutcome::Failed(msg) => Err(anyhow::anyhow!("Reconcile failed: {msg}")),
    }
}

/// Perform a lexical search across indexed files.
pub(crate) fn search(repo_root: &Path, query: &str) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    check_store_ready(&synrepo_dir, &config, StoreId::Index)?;

    let matches = synrepo::substrate::search(&config, repo_root, query)?;
    for search_match in &matches {
        println!(
            "{}:{}: {}",
            search_match.path.display(),
            search_match.line_number,
            String::from_utf8_lossy(&search_match.line_content).trim_end()
        );
    }

    println!("Found {} matches.", matches.len());
    Ok(())
}

/// Execute a graph query and format the output.
pub(crate) fn graph_query(repo_root: &Path, query: &str) -> anyhow::Result<()> {
    println!("{}", graph_query_output(repo_root, query)?);
    Ok(())
}

/// Output graph statistics.
pub(crate) fn graph_stats(repo_root: &Path) -> anyhow::Result<()> {
    println!("{}", graph_stats_output(repo_root)?);
    Ok(())
}

/// Output a specific node's data.
pub(crate) fn node(repo_root: &Path, id: &str) -> anyhow::Result<()> {
    println!("{}", node_output(repo_root, id)?);
    Ok(())
}
