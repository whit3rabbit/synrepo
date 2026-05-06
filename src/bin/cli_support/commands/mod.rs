use std::path::Path;

use synrepo::{config::Config, store::compatibility::StoreId};

use super::graph::{check_store_ready, node_output};

pub(crate) mod agent_hooks;
mod basic;
mod ci_run;
mod compact;
mod context;
mod docs;
mod doctor;
mod export;
mod graph_cmd;
mod handoffs;
mod hooks;
mod links;
pub(crate) mod mcp;
mod mcp_runtime;
mod notes;
mod project;
mod project_prune;
mod remove;
mod repair;
mod server;
mod setup;
mod setup_explain;
mod setup_mcp_backup;
mod status;
mod task_route;
mod uninstall;
mod upgrade;
mod watch;

#[cfg(test)]
pub(crate) use basic::change_risk_output;
pub(crate) use basic::{agent_setup, change_risk, init};
pub(crate) use ci_run::ci_run;
#[cfg(test)]
pub(crate) use ci_run::{ci_run_output, CiRunOptions};
pub(crate) use compact::compact;
pub(crate) use context::{
    bench_context, bench_search, cards_alias, explain_alias, impact_alias, risks_alias,
    stats_context, tests_alias, StatFormat,
};
pub(crate) use docs::docs;
#[cfg(test)]
pub(crate) use docs::{
    docs_clean_output, docs_export_output, docs_import_output, docs_list_output, docs_search_output,
};
pub(crate) use doctor::doctor;
pub(crate) use export::export;
pub(crate) use graph_cmd::graph;
pub(crate) use handoffs::handoffs;
pub(crate) use hooks::install_hooks;
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
pub(crate) use project::{
    project_add, project_inspect, project_list, project_remove, project_rename, project_use,
};
#[cfg(test)]
pub(crate) use project::{
    project_add_output, project_inspect_output, project_list_output, project_remove_output,
    project_rename_output, project_use_output,
};
pub(crate) use project_prune::project_prune_missing;
#[cfg(test)]
pub(crate) use project_prune::project_prune_missing_output;
pub(crate) use remove::remove;
pub(crate) use repair::{check, reconcile, sync};
pub(crate) use server::server;
pub(crate) use setup::{agent_setup_many_resolved, resolve_tool_resolution, setup_many_resolved};
#[cfg(test)]
pub(crate) use setup::{
    classify_mcp_registration, classify_shim_freshness, entry_after_failure, entry_after_success,
    render_client_setup_summary, ClientBefore, ClientSetupEntry, McpRegistration, ShimFreshness,
};
pub(crate) use setup::{
    resolve_setup_scope, step_apply_integration, step_ensure_ready, step_init,
    step_install_agent_hooks, step_register_mcp, step_write_shim, StepOutcome,
};
pub(crate) use setup_explain::step_apply_explain;
pub(crate) use setup_mcp_backup::{mcp_config_has_synrepo, step_backup_mcp_config};
pub(crate) use status::{status, status_output};
pub(crate) use task_route::task_route;
pub(crate) use uninstall::uninstall;
#[cfg(test)]
pub(crate) use upgrade::report_reconcile_outcome;
pub(crate) use upgrade::upgrade;
pub(crate) use watch::{watch, watch_internal, watch_status, watch_stop};

/// Perform a lexical search across indexed files.
#[cfg(test)]
pub(crate) fn search(
    repo_root: &Path,
    query: &str,
    options: syntext::SearchOptions,
) -> anyhow::Result<()> {
    print!("{}", search_output(repo_root, query, options)?);
    Ok(())
}

pub(crate) fn search_with_mode(
    repo_root: &Path,
    query: &str,
    options: syntext::SearchOptions,
    mode: crate::cli_support::cli_args::SearchModeArg,
) -> anyhow::Result<()> {
    print!(
        "{}",
        search_output_with_mode(repo_root, query, options, mode)?
    );
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn search_output(
    repo_root: &Path,
    query: &str,
    options: syntext::SearchOptions,
) -> anyhow::Result<String> {
    search_output_with_mode(
        repo_root,
        query,
        options,
        crate::cli_support::cli_args::SearchModeArg::Lexical,
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn search_output_with_mode(
    repo_root: &Path,
    query: &str,
    options: syntext::SearchOptions,
    mode: crate::cli_support::cli_args::SearchModeArg,
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

    if mode == crate::cli_support::cli_args::SearchModeArg::Lexical {
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
        return Ok(out);
    }

    let report = synrepo::substrate::hybrid_search(&config, repo_root, query, &options)?;
    let mut out = String::new();
    for row in &report.rows {
        let target = row
            .path
            .clone()
            .or_else(|| row.symbol_id.map(|id| id.to_string()))
            .or_else(|| row.chunk_id.clone())
            .unwrap_or_else(|| "<semantic>".to_string());
        let line = row
            .line
            .map(|n| n.to_string())
            .unwrap_or_else(|| "-".to_string());
        let content = row.content.as_deref().unwrap_or("");
        writeln!(out, "{}:{}: {}", target, line, content).unwrap();
    }

    if report.rows.is_empty() {
        writeln!(out, "No matches found for `{query}`.").unwrap();
    } else {
        writeln!(
            out,
            "Found {} matches (engine: {}).",
            report.rows.len(),
            report.engine
        )
        .unwrap();
    }
    Ok(out)
}

/// Output a specific node's data.
pub(crate) fn node(repo_root: &Path, id: &str) -> anyhow::Result<()> {
    println!("{}", node_output(repo_root, id)?);
    Ok(())
}
