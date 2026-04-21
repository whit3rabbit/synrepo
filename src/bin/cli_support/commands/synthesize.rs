//! `synrepo synthesize` — refresh commentary synthesis against stale rows.
//!
//! Mirrors the `RepairAction::RefreshCommentary` code path executed by
//! `synrepo sync`, but lets the operator scope the run to a list of repo-root
//! path prefixes or to hotspots from recent git history. `--dry-run` prints the
//! intersected target set without loading a provider.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use synrepo::{
    config::Config,
    core::ids::NodeId,
    pipeline::{
        git::GitIntelligenceContext,
        git_intelligence::analyze_recent_history,
        maintenance::plan_maintenance,
        repair::{
            normalize_scope_prefixes, path_matches_any_prefix, refresh_commentary,
            resolve_commentary_node, ActionContext,
        },
        writer::{acquire_write_admission, map_lock_error},
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
};

/// Refresh commentary synthesis. Optional `paths`/`changed`/`dry_run` scope the run.
pub(crate) fn synthesize(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("synthesize: not initialized — run `synrepo init` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    let scope = compute_scope(repo_root, &config, paths, changed)?;
    if changed && matches!(&scope, Some(s) if s.is_empty()) {
        println!("No changed files found in last 50 commits; nothing to refresh.");
        return Ok(());
    }

    if dry_run {
        return print_dry_run(&synrepo_dir, scope.as_deref());
    }

    let maint_plan = plan_maintenance(&synrepo_dir, &config);
    let _writer_lock = acquire_write_admission(&synrepo_dir, "synthesize")
        .map_err(|err| map_lock_error("synthesize", err))?;

    let action_context = ActionContext {
        repo_root,
        synrepo_dir: &synrepo_dir,
        config: &config,
        maint_plan: &maint_plan,
    };

    let mut actions_taken: Vec<String> = Vec::new();
    refresh_commentary(&action_context, &mut actions_taken, scope.as_deref())?;

    if actions_taken.is_empty() {
        println!("No actions taken.");
    } else {
        for action in &actions_taken {
            println!("  {action}");
        }
    }
    Ok(())
}

fn compute_scope(
    repo_root: &Path,
    config: &Config,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<Option<Vec<PathBuf>>> {
    if changed {
        let context = GitIntelligenceContext::inspect(repo_root, config);
        let insights = analyze_recent_history(&context, 50, 50)
            .map_err(|e| anyhow::anyhow!("synthesize: cannot sample git history ({e})"))?;
        let hotspot_paths: Vec<PathBuf> = insights
            .hotspots
            .iter()
            .map(|h| PathBuf::from(&h.path))
            .collect();
        Ok(Some(hotspot_paths))
    } else if paths.is_empty() {
        Ok(None)
    } else {
        Ok(Some(paths.into_iter().map(PathBuf::from).collect()))
    }
}

fn print_dry_run(synrepo_dir: &Path, scope: Option<&[PathBuf]>) -> anyhow::Result<()> {
    let overlay_db = SqliteOverlayStore::db_path(&synrepo_dir.join("overlay"));
    if !overlay_db.exists() {
        println!("Overlay has not been materialized yet; no commentary to refresh.");
        return Ok(());
    }

    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay"))
        .map_err(|e| anyhow::anyhow!("synthesize --dry-run: cannot open overlay ({e})"))?;
    let rows = overlay
        .commentary_hashes()
        .map_err(|e| anyhow::anyhow!("synthesize --dry-run: cannot read commentary ({e})"))?;

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))
        .map_err(|e| anyhow::anyhow!("synthesize --dry-run: cannot open graph ({e})"))?;

    let prefixes = scope.map(normalize_scope_prefixes);
    let mut stale_in_scope: Vec<String> = Vec::new();

    for (node_id_str, stored_hash) in rows {
        let Ok(node_id) = NodeId::from_str(&node_id_str) else {
            continue;
        };
        let Some(snap) = resolve_commentary_node(&graph, node_id)? else {
            continue;
        };
        if snap.content_hash == stored_hash {
            continue;
        }
        if let Some(p) = &prefixes {
            if !path_matches_any_prefix(&snap.file.path, p) {
                continue;
            }
        }
        stale_in_scope.push(snap.file.path);
    }

    stale_in_scope.sort();
    stale_in_scope.dedup();

    if stale_in_scope.is_empty() {
        match scope {
            Some(_) => println!("No stale commentary entries in scope."),
            None => println!("No stale commentary entries."),
        }
    } else {
        println!("Planned refresh targets ({}):", stale_in_scope.len());
        for path in &stale_in_scope {
            println!("  {path}");
        }
    }
    Ok(())
}
