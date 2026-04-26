use std::path::Path;

use synrepo::{config::Config, store::compatibility::StoreId};

use super::graph::{check_store_ready, graph_query_output, graph_stats_output, node_output};

mod basic;
mod ci_run;
mod compact;
mod context;
mod doctor;
mod export;
mod handoffs;
mod links;
pub(crate) mod mcp;
mod mcp_runtime;
mod notes;
mod remove;
mod repair;
mod server;
mod setup;
mod setup_explain;
mod setup_mcp_backup;
mod status;
mod upgrade;
mod watch;

#[cfg(test)]
pub(crate) use basic::change_risk_output;
pub(crate) use basic::{agent_setup, change_risk, init, install_hooks};
pub(crate) use ci_run::ci_run;
#[cfg(test)]
pub(crate) use ci_run::{ci_run_output, CiRunOptions};
pub(crate) use compact::compact;
pub(crate) use context::{
    bench_context, cards_alias, explain_alias, impact_alias, risks_alias, stats_context,
    tests_alias, StatFormat,
};
pub(crate) use doctor::doctor;
pub(crate) use export::export;
pub(crate) use handoffs::handoffs;
pub(crate) use links::{findings, links_accept, links_list, links_reject, links_review};
#[cfg(test)]
pub(crate) use links::{
    findings_output, links_accept_commit, links_list_output, links_review_output, CommitArgs,
    LinksCommitStore, RealLinksStore,
};
#[cfg(test)]
pub(crate) use mcp_runtime::prepare_state as prepare_mcp_state;
pub(crate) use mcp_runtime::run_mcp_server;
pub(crate) use notes::{
    notes_add, notes_audit, notes_forget, notes_link, notes_list, notes_supersede, notes_verify,
};
#[cfg(test)]
pub(crate) use notes::{notes_add_output, notes_list_output};
pub(crate) use remove::remove;
pub(crate) use repair::{check, reconcile, sync};
pub(crate) use server::server;
pub(crate) use setup::{agent_setup_many_resolved, resolve_tool_resolution, setup_many_resolved};
#[cfg(test)]
pub(crate) use setup::{
    classify_mcp_registration, classify_shim_freshness, entry_after_failure, entry_after_success,
    render_client_setup_summary, setup_claude_mcp, setup_codex_mcp, setup_cursor_mcp,
    setup_opencode_mcp, setup_roo_mcp, setup_windsurf_mcp, ClientBefore, ClientSetupEntry,
    McpRegistration, ShimFreshness, StepOutcome,
};
pub(crate) use setup::{
    step_apply_integration, step_ensure_ready, step_init, step_register_mcp, step_write_shim,
};
pub(crate) use setup_explain::step_apply_explain;
pub(crate) use setup_mcp_backup::{mcp_config_has_synrepo, step_backup_mcp_config};
pub(crate) use status::{status, status_output};
#[cfg(test)]
pub(crate) use upgrade::report_reconcile_outcome;
pub(crate) use upgrade::upgrade;
pub(crate) use watch::{watch, watch_internal, watch_status, watch_stop};

/// Perform a lexical search across indexed files.
pub(crate) fn search(
    repo_root: &Path,
    query: &str,
    options: syntext::SearchOptions,
) -> anyhow::Result<()> {
    print!("{}", search_output(repo_root, query, options)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn search_output(
    repo_root: &Path,
    query: &str,
    options: syntext::SearchOptions,
) -> anyhow::Result<String> {
    use std::fmt::Write as _;

    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    check_store_ready(&synrepo_dir, &config, StoreId::Index)?;
    if let Some(pid) = synrepo::pipeline::writer::live_owner_pid(&synrepo_dir) {
        anyhow::bail!(
            "search is unavailable while writer lock is held by pid {pid}. Wait for the active write to finish, then retry."
        );
    }

    let matches = synrepo::substrate::search_with_options(&config, repo_root, query, &options)?;
    let mut out = String::new();
    for search_match in &matches {
        writeln!(
            out,
            "{}:{}: {}",
            search_match.path.display(),
            search_match.line_number,
            String::from_utf8_lossy(&search_match.line_content).trim_end()
        )
        .unwrap();
    }

    if matches.is_empty() {
        writeln!(out, "No matches found for `{query}`.").unwrap();
    } else {
        writeln!(out, "Found {} matches.", matches.len()).unwrap();
    }
    Ok(out)
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
