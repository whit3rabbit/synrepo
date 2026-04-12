//! Bootstrap orchestrator: creates or repairs the `.synrepo/` runtime layout.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::config::{Config, Mode};
use crate::pipeline::structural::run_structural_compile;
use crate::pipeline::writer::{acquire_writer_lock, LockError};
use crate::store::compatibility::{self, CompatibilityReport};
use crate::store::sqlite::SqliteGraphStore;

use super::mode_inspect::inspect_repository_mode;
use super::report::{BootstrapAction, BootstrapHealth, BootstrapReport};

#[cfg(test)]
mod tests;

static NEXT_BOOTSTRAP_TMP_ID: AtomicU64 = AtomicU64::new(0);

/// Run bootstrap for the given repository root, optionally forcing a mode.
pub fn bootstrap(
    repo_root: &Path,
    requested_mode: Option<Mode>,
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
    if compatibility_report.has_blocking_actions() {
        return Err(blocked_by_compatibility(
            &synrepo_dir,
            &compatibility_report,
        ));
    }

    // Acquire the exclusive writer lock before any state mutation. Held until
    // the end of `bootstrap()` via RAII drop. Fails fast if another live
    // process already holds the lock.
    let _lock = acquire_writer_lock(&synrepo_dir).map_err(|err| match err {
        LockError::HeldByOther { pid, .. } => anyhow::anyhow!(
            "Bootstrap blocked: writer lock held by pid {pid}. \
             Wait for that process to finish, then rerun `synrepo init`."
        ),
        LockError::Io { path, source } => {
            anyhow::anyhow!(
                "Failed to acquire writer lock at {}: {source}",
                path.display()
            )
        }
    })?;

    let layout_changed = compatibility::ensure_runtime_layout(&synrepo_dir)?;
    let remediated = compatibility::apply_runtime_actions(&synrepo_dir, &compatibility_report)?;

    atomic_write_file(&config_path, toml::to_string_pretty(&config)?.as_bytes())?;
    write_synrepo_gitignore(&synrepo_dir)?;
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
    let mut graph = SqliteGraphStore::open(&graph_dir)?;
    let compile = run_structural_compile(repo_root, &config, &mut graph)?;

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

    Ok(BootstrapReport {
        health,
        mode,
        mode_guidance,
        compatibility_guidance: compatibility_report.guidance_lines(),
        synrepo_dir,
        substrate_status,
        graph_status,
        next_step,
    })
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
        b"# Gitignore everything in .synrepo/ except config.toml\n\
         *\n\
         !.gitignore\n\
         !config.toml\n",
    )?;
    Ok(())
}

fn atomic_write_file(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        return Err(anyhow::anyhow!(
            "cannot atomically write {} without a parent directory",
            path.display()
        ));
    };
    fs::create_dir_all(parent)?;

    let tmp_path = atomic_write_tmp_path(path);
    if let Err(error) = fs::write(&tmp_path, contents).and_then(|_| fs::rename(&tmp_path, path)) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error.into());
    }

    Ok(())
}

fn atomic_write_tmp_path(path: &Path) -> PathBuf {
    let id = NEXT_BOOTSTRAP_TMP_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bootstrap");
    path.with_file_name(format!("{file_name}.tmp.{}.{}", std::process::id(), id))
}

fn blocked_by_compatibility(synrepo_dir: &Path, report: &CompatibilityReport) -> anyhow::Error {
    let issue = report.guidance_lines().join("\nCompatibility: ");
    anyhow::anyhow!(
        "Bootstrap health: blocked\nRuntime path: {}\nIssue: storage compatibility requires manual intervention\nCompatibility: {}\nNext: resolve or remove the incompatible runtime state, then rerun `synrepo init`.",
        synrepo_dir.display(),
        issue,
    )
}
