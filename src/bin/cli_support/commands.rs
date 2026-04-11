use std::path::Path;

use synrepo::{
    config::{Config, Mode},
    pipeline::watch::{persist_reconcile_state, run_reconcile_pass, ReconcileOutcome},
    store::compatibility::StoreId,
};

use super::graph::{check_store_ready, graph_query_output, graph_stats_output, node_output};

pub(crate) fn init(repo_root: &Path, requested_mode: Option<Mode>) -> anyhow::Result<()> {
    let report = synrepo::bootstrap::bootstrap(repo_root, requested_mode)?;
    print!("{}", report.render());
    Ok(())
}

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

pub(crate) fn graph_query(repo_root: &Path, query: &str) -> anyhow::Result<()> {
    println!("{}", graph_query_output(repo_root, query)?);
    Ok(())
}

pub(crate) fn graph_stats(repo_root: &Path) -> anyhow::Result<()> {
    println!("{}", graph_stats_output(repo_root)?);
    Ok(())
}

pub(crate) fn node(repo_root: &Path, id: &str) -> anyhow::Result<()> {
    println!("{}", node_output(repo_root, id)?);
    Ok(())
}
