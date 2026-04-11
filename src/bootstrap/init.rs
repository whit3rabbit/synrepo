//! Bootstrap orchestrator: creates or repairs the `.synrepo/` runtime layout.

use std::path::Path;

use crate::config::{Config, Mode};
use crate::store::compatibility::{self, CompatibilityReport};

use super::mode_inspect::inspect_repository_mode;
use super::report::{BootstrapAction, BootstrapHealth, BootstrapReport};

/// Run bootstrap for the given repository root, optionally forcing a mode.
pub fn bootstrap(repo_root: &Path, requested_mode: Option<Mode>) -> anyhow::Result<BootstrapReport> {
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
        return Err(blocked_by_compatibility(&synrepo_dir, &compatibility_report));
    }

    let layout_changed = compatibility::ensure_runtime_layout(&synrepo_dir)?;
    let remediated = compatibility::apply_runtime_actions(&synrepo_dir, &compatibility_report)?;

    std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
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
    // Write a default .gitignore for .synrepo/
    let gitignore_path = synrepo_dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        "# Gitignore everything in .synrepo/ except config.toml\n\
         *\n\
         !.gitignore\n\
         !config.toml\n",
    )?;
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

#[cfg(test)]
mod tests {
    use super::bootstrap;
    use crate::bootstrap::BootstrapHealth;
    use crate::config::{Config, Mode};
    use crate::store::compatibility::{self, StoreId};
    use tempfile::tempdir;

    #[test]
    fn bootstrap_fresh_repo_reports_healthy_summary() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "fresh token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();

        assert_eq!(report.health, BootstrapHealth::Healthy);
        assert_eq!(report.mode, Mode::Auto);
        assert!(rendered.contains("Bootstrap health: healthy"));
        assert!(rendered.contains("Mode: Auto"));
        assert!(rendered.contains("Mode guidance: no rationale markdown was found"));
        assert!(rendered.contains("Runtime path:"));
        assert!(rendered.contains("Substrate: built initial index"));
        assert!(rendered.contains("Next: run `synrepo search <query>`"));
        assert!(compatibility::snapshot_path(&Config::synrepo_dir(repo.path())).exists());
    }

    #[test]
    fn bootstrap_selects_curated_when_rationale_markdown_exists() {
        let repo = tempdir().unwrap();
        let adr_dir = repo.path().join("docs/adr");
        std::fs::create_dir_all(&adr_dir).unwrap();
        std::fs::write(adr_dir.join("0001-record.md"), "# Decision\n").unwrap();
        std::fs::write(repo.path().join("README.md"), "curated token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();
        let config = Config::load(repo.path()).unwrap();

        assert_eq!(report.mode, Mode::Curated);
        assert_eq!(config.mode, Mode::Curated);
        assert!(rendered.contains("Mode guidance: repository inspection selected Curated"));
        assert!(rendered.contains("docs/adr"));
    }

    #[test]
    fn bootstrap_rerun_refreshes_existing_runtime() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "before refresh\n").unwrap();
        bootstrap(repo.path(), None).unwrap();

        std::fs::write(repo.path().join("README.md"), "after refresh token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let matches = crate::substrate::search(
            &Config::load(repo.path()).unwrap(),
            repo.path(),
            "after refresh token",
        )
        .unwrap();

        assert_eq!(report.health, BootstrapHealth::Healthy);
        assert!(report.substrate_status.contains("refreshed existing index"));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn bootstrap_repairs_partial_runtime_and_reports_degraded() {
        let repo = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(repo.path());
        std::fs::create_dir_all(&synrepo_dir).unwrap();
        std::fs::write(repo.path().join("README.md"), "repair token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();

        assert_eq!(report.health, BootstrapHealth::Degraded);
        assert!(rendered.contains("Bootstrap health: degraded"));
        assert!(rendered.contains("repaired runtime state and rebuilt index"));
        assert!(synrepo_dir.join("index/manifest.json").exists());
    }

    #[test]
    fn bootstrap_reports_graph_sensitive_config_drift_without_blocking() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "compat token\n").unwrap();
        bootstrap(repo.path(), None).unwrap();

        let updated = Config {
            concept_directories: vec![
                "docs/concepts".to_string(),
                "docs/adr".to_string(),
                "docs/decisions".to_string(),
                "architecture/decisions".to_string(),
            ],
            ..Config::load(repo.path()).unwrap()
        };
        std::fs::write(
            Config::synrepo_dir(repo.path()).join("config.toml"),
            toml::to_string_pretty(&updated).unwrap(),
        )
        .unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();

        assert!(rendered.contains("Compatibility:"));
        assert!(rendered.contains("concept_directories"));
    }

    #[test]
    fn bootstrap_blocks_on_invalid_existing_config() {
        let repo = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(repo.path());
        std::fs::create_dir_all(&synrepo_dir).unwrap();
        std::fs::write(synrepo_dir.join("config.toml"), "mode = [not valid").unwrap();

        let error = bootstrap(repo.path(), None).unwrap_err().to_string();

        assert!(error.contains("Bootstrap health: blocked"));
        assert!(error.contains("invalid existing config"));
    }

    #[test]
    fn bootstrap_explicit_mode_overrides_existing_config_on_refresh() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "mode token\n").unwrap();
        bootstrap(repo.path(), Some(Mode::Curated)).unwrap();

        let report = bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
        let config = Config::load(repo.path()).unwrap();

        assert_eq!(report.mode, Mode::Auto);
        assert_eq!(config.mode, Mode::Auto);
    }

    #[test]
    fn bootstrap_honors_explicit_auto_with_curated_recommendation() {
        let repo = tempdir().unwrap();
        let adr_dir = repo.path().join("docs/adr");
        std::fs::create_dir_all(&adr_dir).unwrap();
        std::fs::write(adr_dir.join("0002-architecture.md"), "# Architecture\n").unwrap();
        std::fs::write(repo.path().join("README.md"), "explicit token\n").unwrap();

        let report = bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
        let rendered = report.render();
        let config = Config::load(repo.path()).unwrap();

        assert_eq!(report.mode, Mode::Auto);
        assert_eq!(config.mode, Mode::Auto);
        assert!(rendered.contains("Mode guidance: repository inspection suggests Curated"));
        assert!(rendered.contains("keeping explicit Auto"));
    }

    #[test]
    fn bootstrap_blocks_on_newer_graph_store_version() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "graph token\n").unwrap();
        bootstrap(repo.path(), None).unwrap();

        let synrepo_dir = Config::synrepo_dir(repo.path());
        std::fs::write(synrepo_dir.join("graph/nodes.db"), "db").unwrap();
        let mut snapshot = compatibility::write_runtime_snapshot(
            &synrepo_dir,
            &Config::load(repo.path()).unwrap(),
        )
        .unwrap();
        snapshot
            .store_format_versions
            .insert(StoreId::Graph.as_str().to_string(), 2);
        std::fs::write(
            compatibility::snapshot_path(&synrepo_dir),
            serde_json::to_vec_pretty(&snapshot).unwrap(),
        )
        .unwrap();

        let error = bootstrap(repo.path(), None).unwrap_err().to_string();

        assert!(error.contains("Bootstrap health: blocked"));
        assert!(error.contains("graph"));
        assert!(error.contains("block"));
    }
}
