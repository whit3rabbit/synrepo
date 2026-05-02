//! Bootstrap orchestrator: creates or repairs the `.synrepo/` runtime layout.

use std::{fs, path::Path};

use crate::config::{Config, Mode};
use crate::pipeline::structural::run_structural_compile;
use crate::pipeline::watch::{
    emit_cochange_edges_pass, finish_runtime_surfaces, persist_reconcile_state, ReconcileOutcome,
    RepoIndexStrategy,
};
use crate::pipeline::writer::{acquire_writer_lock, LockError};
use crate::store::compatibility::{self, CompatibilityReport};
use crate::store::sqlite::SqliteGraphStore;
use crate::util::atomic_write;

use super::mode_inspect::inspect_repository_mode;
use super::report::{BootstrapAction, BootstrapHealth, BootstrapReport, DegradedCapability};
use super::runtime_probe::probe;
use crate::surface::readiness::ReadinessMatrix;
use crate::surface::status_snapshot::{build_status_snapshot, StatusOptions};

#[cfg(test)]
mod force_tests;
#[cfg(test)]
mod tests;

/// Run bootstrap for the given repository root, optionally forcing a mode.
pub fn bootstrap(
    repo_root: &Path,
    requested_mode: Option<Mode>,
    update_gitignore: bool,
) -> anyhow::Result<BootstrapReport> {
    bootstrap_with_force(repo_root, requested_mode, update_gitignore, false)
}

/// Like [`bootstrap`], but with `force = true` clears any blocked canonical
/// stores in place before continuing. Use only when the operator has
/// explicitly opted into a destructive recreate (e.g. `synrepo init --force`
/// or the repair wizard's `RecreateRuntime` action). The writer-lock and
/// watch-active gates are still enforced.
pub fn bootstrap_with_force(
    repo_root: &Path,
    requested_mode: Option<Mode>,
    update_gitignore: bool,
    force: bool,
) -> anyhow::Result<BootstrapReport> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let runtime_already_existed = synrepo_dir.exists();
    let config_path = synrepo_dir.join("config.toml");
    let existing_config = load_existing_config(&config_path, &synrepo_dir)?;
    let had_existing_config = existing_config.is_some();
    let gitignore_path = synrepo_dir.join(".gitignore");
    let had_gitignore = gitignore_path.exists();
    let inspection_config = existing_config.clone().unwrap_or_default();
    let inspection = inspect_repository_mode(repo_root, &inspection_config)?;
    let mode = requested_mode
        .or(existing_config.as_ref().map(|config| config.mode))
        .unwrap_or(inspection.recommended_mode);
    let mode_guidance = inspection.guidance_for(requested_mode, existing_config.as_ref(), mode);
    let config = existing_config.unwrap_or_else(|| Config {
        mode,
        ..Config::default()
    });
    let config = Config { mode, ..config };
    let compatibility_report =
        compatibility::evaluate_runtime(&synrepo_dir, runtime_already_existed, &config)?;
    if compatibility_report.has_blocking_actions() && !force {
        return Err(blocked_by_compatibility(
            &synrepo_dir,
            &compatibility_report,
        ));
    }

    // Acquire the exclusive writer lock before any state mutation. Held until
    // the end of `bootstrap()` via RAII drop. Fails fast if another live
    // process already holds the lock. `--force` does NOT bypass this gate:
    // recreating the runtime while another process is mutating it would
    // corrupt the live writer's view.
    let _lock = acquire_writer_lock(&synrepo_dir).map_err(|err| match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "Bootstrap blocked: writer lock held by pid {pid}. \
             Wait for that process to finish, then rerun `synrepo init`."
        ),
        LockError::WatchOwned { watch_pid } => anyhow::anyhow!(
            "Bootstrap blocked: watch service is active (pid {watch_pid}); \
             run `synrepo watch stop` before re-running `synrepo init`."
        ),
        LockError::WatchStarting => anyhow::anyhow!(
            "Bootstrap blocked: watch service is still starting; wait for it to become ready, \
             then rerun `synrepo init`."
        ),
        LockError::WrongThread { .. } => anyhow::anyhow!(
            "Bootstrap blocked: writer lock already held by another thread in this process."
        ),
        LockError::Malformed { lock_path, detail } => anyhow::anyhow!(
            "Bootstrap blocked: writer lock at {} is malformed ({detail}); remove the file and rerun `synrepo init`.",
            lock_path.display()
        ),
        LockError::Io { path, source } => anyhow::anyhow!(
            "Failed to acquire writer lock at {}: {source}",
            path.display()
        ),
    })?;

    // Force path: clear any blocked canonical stores under the writer lock,
    // then re-evaluate so the rest of bootstrap sees a clean report. The
    // re-evaluation must come back blocking-free; if it does not, the
    // runtime is in a state we cannot recover from automatically.
    let compatibility_report = if force && compatibility_report.has_blocking_actions() {
        compatibility::clear_blocked_stores(&_lock, &synrepo_dir, &compatibility_report)?;
        let report = compatibility::evaluate_runtime(&synrepo_dir, synrepo_dir.exists(), &config)?;
        if report.has_blocking_actions() {
            return Err(blocked_by_compatibility(&synrepo_dir, &report));
        }
        report
    } else {
        compatibility_report
    };

    let layout_changed = compatibility::ensure_runtime_layout(&synrepo_dir)?;
    let remediated =
        compatibility::apply_runtime_actions(&_lock, &synrepo_dir, &compatibility_report)?;

    atomic_write_file(&config_path, toml::to_string_pretty(&config)?.as_bytes())?;
    write_synrepo_gitignore(&synrepo_dir)?;
    let mut root_gitignore_added_now = false;
    if update_gitignore {
        root_gitignore_added_now = append_to_root_gitignore(repo_root)?;
    }
    let repaired = runtime_already_existed
        && (layout_changed || remediated || !had_existing_config || !had_gitignore);
    let action = if !runtime_already_existed {
        BootstrapAction::Created
    } else if repaired {
        BootstrapAction::Repaired
    } else {
        BootstrapAction::Refreshed
    };

    let build_report = crate::substrate::build_index(&config, repo_root)?;

    let graph_dir = synrepo_dir.join("graph");
    // If a `nodes.db` already exists at the canonical path, refuse to open it
    // for write until we know it is a real SQLite database. The probe rejects
    // an unopenable file as `Missing::GraphStore`, so the user reaches this
    // path via the "run `synrepo init`" repair guidance — but `open()` would
    // otherwise surface a raw rusqlite "file is not a database" error from
    // `init_schema`'s first PRAGMA, with no actionable next step.
    let db_path = SqliteGraphStore::db_path(&graph_dir);
    if db_path.exists() {
        if let Err(err) = SqliteGraphStore::validate_existing(&graph_dir) {
            return Err(anyhow::anyhow!(
                "Existing graph store at {} is not an openable SQLite database ({err}). \
                 Remove `.synrepo/graph/` (or the whole `.synrepo/`) and rerun `synrepo init` to start fresh.",
                db_path.display()
            ));
        }
    }
    let mut graph = SqliteGraphStore::open(&graph_dir)?;
    let compile = run_structural_compile(repo_root, &config, &mut graph)?;

    // Co-change edge emission is best-effort during bootstrap. Failure is
    // non-fatal: the graph is structurally complete without co-change edges.
    if let Err(err) = emit_cochange_edges_pass(repo_root, &config, &mut graph) {
        tracing::warn!(error = %err, "co-change edge emission skipped during bootstrap");
    }
    finish_runtime_surfaces(
        repo_root,
        &config,
        &synrepo_dir,
        &graph,
        RepoIndexStrategy::Skip,
    )?;

    // Bootstrap is a reconcile pass: persist a reconcile-state record so the
    // readiness matrix does not report `stale, no reconcile recorded`
    // immediately after a successful first-time init.
    persist_reconcile_state(
        &synrepo_dir,
        &ReconcileOutcome::Completed(compile.clone()),
        0,
    );

    compatibility::write_runtime_snapshot(&synrepo_dir, &config)?;
    let health = match action {
        BootstrapAction::Repaired => BootstrapHealth::Degraded,
        BootstrapAction::Created | BootstrapAction::Refreshed => BootstrapHealth::Healthy,
    };
    let substrate_status = match action {
        BootstrapAction::Created => format!(
            "built initial index with {} discovered files",
            build_report.indexed_files
        ),
        BootstrapAction::Refreshed => format!(
            "refreshed existing index with {} discovered files",
            build_report.indexed_files
        ),
        BootstrapAction::Repaired => format!(
            "repaired runtime state and rebuilt index with {} discovered files",
            build_report.indexed_files
        ),
    };
    let graph_verb = if action == BootstrapAction::Created {
        "populated"
    } else {
        "refreshed"
    };
    let graph_status = format!(
        "{graph_verb} graph: {} file nodes, {} symbols, {} concept nodes",
        compile.files_discovered, compile.symbols_extracted, compile.concept_nodes_emitted,
    );
    let next_step = match health {
        BootstrapHealth::Healthy => {
            "run `synrepo search <query>` to inspect the lexical index".to_string()
        }
        BootstrapHealth::Degraded => {
            "review the repaired runtime state, then run `synrepo search <query>`".to_string()
        }
    };

    // Record the install in `~/.synrepo/projects.toml` so `synrepo remove`
    // can undo exactly what we did. Best-effort: a write failure (missing
    // home dir, read-only home, etc.) must not fail the bootstrap itself
    // because the removal path has a filesystem-scan fallback.
    if let Err(err) = crate::registry::record_install(repo_root, root_gitignore_added_now) {
        tracing::warn!(error = %err, "install registry update skipped during bootstrap");
    }

    // Build the capability readiness matrix once the graph has been
    // compiled and state is consistent. The caller's success output can then
    // label any optional feature that is off (e.g. no git history, embeddings
    // disabled) and any core feature that needs a follow-up.
    let degraded_capabilities = collect_degraded_capabilities(repo_root, &synrepo_dir, &config);

    Ok(BootstrapReport {
        health,
        action,
        mode,
        mode_guidance,
        compatibility_guidance: compatibility_report.guidance_lines(),
        synrepo_dir,
        substrate_status,
        graph_status,
        next_step,
        degraded_capabilities,
    })
}

fn collect_degraded_capabilities(
    repo_root: &Path,
    _synrepo_dir: &Path,
    config: &Config,
) -> Vec<DegradedCapability> {
    let snapshot = build_status_snapshot(
        repo_root,
        StatusOptions {
            recent: false,
            full: false,
        },
    );
    if !snapshot.initialized {
        return Vec::new();
    }
    let probe_report = probe(repo_root);
    let matrix = ReadinessMatrix::build(repo_root, &probe_report, &snapshot, config);
    matrix
        .degraded_rows()
        .map(|row| DegradedCapability {
            capability: row.capability.as_str().to_string(),
            state: row.state.as_str().to_string(),
            detail: row.detail.clone(),
            next_action: row.next_action.clone(),
        })
        .collect()
}

fn load_existing_config(config_path: &Path, synrepo_dir: &Path) -> anyhow::Result<Option<Config>> {
    if !config_path.exists() {
        return Ok(None);
    }

    fn blocked(synrepo_dir: &Path, issue: String, next: &str) -> anyhow::Error {
        anyhow::anyhow!(
            "Bootstrap health: blocked\nRuntime path: {}\nIssue: {issue}\nNext: {next}",
            synrepo_dir.display(),
        )
    }

    let text = std::fs::read_to_string(config_path).map_err(|error| {
        blocked(
            synrepo_dir,
            format!("failed to read existing config: {error}"),
            &format!(
                "fix or remove {} and rerun `synrepo init`.",
                config_path.display()
            ),
        )
    })?;
    toml::from_str(&text).map(Some).map_err(|error| {
        blocked(
            synrepo_dir,
            format!(
                "invalid existing config at {}: {error}",
                config_path.display()
            ),
            "fix or remove it, then rerun `synrepo init`.",
        )
    })
}

fn write_synrepo_gitignore(synrepo_dir: &Path) -> anyhow::Result<()> {
    let gitignore_path = synrepo_dir.join(".gitignore");
    atomic_write_file(
        &gitignore_path,
        b"# Gitignore everything in .synrepo/\n\
         *\n\
         !.gitignore\n\
         # Generated vectors directory (semantic-triage)\n\
         index/vectors/\n",
    )?;
    Ok(())
}

/// Append `.synrepo/` to the root `.gitignore` if it is not already present.
///
/// Returns `true` when this call actually wrote the line (so the caller can
/// record ownership in the install registry), `false` when the user's
/// `.gitignore` already contained the entry (in which case the user owns the
/// line and the removal path must not strip it).
fn append_to_root_gitignore(repo_root: &Path) -> anyhow::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    let entry = ".synrepo/";

    if gitignore_path.exists() {
        let content = std::fs::read_to_string(&gitignore_path)?;
        if content.lines().any(|l| l.trim() == entry) {
            return Ok(false);
        }
        let mut new_content = content;
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(entry);
        new_content.push('\n');
        std::fs::write(&gitignore_path, new_content)?;
    } else {
        std::fs::write(&gitignore_path, format!("{entry}\n"))?;
    }
    Ok(true)
}

/// Remove a line matching `entry` from the root `.gitignore`.
///
/// Strictly line-exact (trim-compared) so neighbouring lines the user wrote
/// are preserved byte-for-byte. Returns `true` when a line was stripped,
/// `false` when no matching line existed. Missing `.gitignore` is not an
/// error — it yields `false`.
///
/// Invariant: callers should only invoke this when the install registry
/// recorded `root_gitignore_entry_added = true` (or the export equivalent).
/// Stripping a line we did not add is how users lose their config.
pub fn remove_from_root_gitignore(repo_root: &Path, entry: &str) -> anyhow::Result<bool> {
    let gitignore_path = repo_root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(&gitignore_path)?;
    let had_trailing_newline = content.ends_with('\n');
    let mut removed = false;
    let mut kept = Vec::with_capacity(content.lines().count());
    for line in content.lines() {
        if !removed && line.trim() == entry {
            removed = true;
            continue;
        }
        kept.push(line);
    }
    if !removed {
        return Ok(false);
    }
    let mut new_content = kept.join("\n");
    if had_trailing_newline && !new_content.is_empty() {
        new_content.push('\n');
    }
    std::fs::write(&gitignore_path, new_content)?;
    Ok(true)
}

/// Delegate to the canonical `util::atomic_write`, but ensure the parent
/// directory exists first (callers in this module occasionally target
/// a freshly-created `.synrepo/`).
fn atomic_write_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(path, contents)?;
    Ok(())
}

fn blocked_by_compatibility(synrepo_dir: &Path, report: &CompatibilityReport) -> anyhow::Error {
    let issue = report.guidance_lines().join("\nCompatibility: ");
    anyhow::anyhow!(
        "Bootstrap health: blocked\nRuntime path: {}\nIssue: storage compatibility requires manual intervention\nCompatibility: {}\nNext: resolve or remove the incompatible runtime state, then rerun `synrepo init`.",
        synrepo_dir.display(),
        issue,
    )
}
