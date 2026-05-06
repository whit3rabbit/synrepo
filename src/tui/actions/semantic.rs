use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use crate::config::Config;
use crate::pipeline::writer::acquire_write_admission;

use super::helpers::{load_repo_config, lock_error_to_action};
use super::{ActionContext, ActionOutcome};

/// Persist the repo-local semantic-triage opt-in flag.
///
/// This intentionally does not run reconcile. Enabling embeddings can trigger
/// model downloads or many local embedding calls during the next reconcile, so
/// the operator chooses that follow-up explicitly with `R` or the CLI.
pub fn set_semantic_triage(ctx: &ActionContext, desired: bool) -> ActionOutcome {
    if desired && !semantic_feature_compiled() {
        return ActionOutcome::Error {
            message: "embeddings are optional; this binary was not built with `semantic-triage`"
                .to_string(),
        };
    }

    let config = match load_repo_config(ctx, "embeddings") {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    if !desired && config.enable_semantic_triage && matches!(global_semantic_enabled(), Ok(true)) {
        return ActionOutcome::Error {
            message: "embeddings are enabled in ~/.synrepo/config.toml; remove that global opt-in before disabling this repo".to_string(),
        };
    }

    if config.enable_semantic_triage == desired {
        return ActionOutcome::Completed {
            message: semantic_message(desired, &config),
        };
    }

    let _lock = match acquire_write_admission(&ctx.synrepo_dir, "embeddings") {
        Ok(lock) => lock,
        Err(err) => return lock_error_to_action(&ctx.synrepo_dir, err),
    };

    let path = ctx.synrepo_dir.join("config.toml");
    match patch_semantic_enabled(&path, desired)
        .and_then(|_| Config::load(&ctx.repo_root).map_err(anyhow::Error::from))
    {
        Ok(updated) if updated.enable_semantic_triage == desired => ActionOutcome::Completed {
            message: semantic_message(desired, &updated),
        },
        Ok(_) => ActionOutcome::Error {
            message: "repo config was written, but merged config did not change; check ~/.synrepo/config.toml".to_string(),
        },
        Err(err) => ActionOutcome::Error {
            message: format!("embeddings config update failed: {err:#}"),
        },
    }
}

fn semantic_feature_compiled() -> bool {
    cfg!(feature = "semantic-triage")
}

fn semantic_message(enabled: bool, config: &Config) -> String {
    if enabled {
        format!(
            "embeddings enabled ({}, {} {}d); run `synrepo reconcile` to build vectors",
            config.semantic_embedding_provider.as_str(),
            config.semantic_model,
            config.embedding_dim
        )
    } else {
        "embeddings disabled; search and routing will use lexical fallback".to_string()
    }
}

fn patch_semantic_enabled(path: &Path, desired: bool) -> anyhow::Result<()> {
    let mut doc = load_toml_document(path)?;
    doc.insert(
        "enable_semantic_triage",
        Item::Value(TomlValue::from(desired)),
    );
    write_toml_document(path, &doc)
}

fn global_semantic_enabled() -> anyhow::Result<bool> {
    let path = Config::global_config_path();
    if !path.exists() {
        return Ok(false);
    }
    let doc = load_toml_document(&path)?;
    Ok(doc
        .get("enable_semantic_triage")
        .and_then(|item| item.as_bool())
        .unwrap_or(false))
}

fn load_toml_document(path: &Path) -> anyhow::Result<DocumentMut> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    raw.parse().map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid TOML ({err})",
            path.display()
        )
    })
}

fn write_toml_document(path: &Path, doc: &DocumentMut) -> anyhow::Result<()> {
    crate::util::atomic_write(path, doc.to_string().as_bytes())
        .with_context(|| format!("failed to atomically write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn isolated_ready_repo() -> (
        crate::test_support::GlobalTestLock,
        tempfile::TempDir,
        crate::config::test_home::HomeEnvGuard,
        tempfile::TempDir,
    ) {
        let lock =
            crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let repo = tempdir().unwrap();
        crate::bootstrap::bootstrap(repo.path(), None, false).expect("bootstrap");
        (lock, repo, guard, home)
    }

    #[test]
    fn disabling_semantic_triage_patches_repo_config() {
        let (_lock, repo, _guard, _home) = isolated_ready_repo();
        let path = Config::synrepo_dir(repo.path()).join("config.toml");
        let mut config = Config::load(repo.path()).unwrap();
        config.enable_semantic_triage = true;
        std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

        let outcome = set_semantic_triage(&ActionContext::new(repo.path()), false);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
        assert!(!Config::load(repo.path()).unwrap().enable_semantic_triage);
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("enable_semantic_triage = false"));
    }

    #[test]
    fn disabling_reports_global_opt_in() {
        let (_lock, repo, _guard, home) = isolated_ready_repo();
        std::fs::create_dir_all(home.path().join(".synrepo")).unwrap();
        std::fs::write(
            home.path().join(".synrepo/config.toml"),
            "enable_semantic_triage = true\n",
        )
        .unwrap();

        let outcome = set_semantic_triage(&ActionContext::new(repo.path()), false);
        assert!(
            matches!(outcome, ActionOutcome::Error { ref message } if message.contains("global opt-in")),
            "got {outcome:?}"
        );
    }

    #[test]
    #[cfg(feature = "semantic-triage")]
    fn enabling_semantic_triage_patches_repo_config_when_feature_exists() {
        let (_lock, repo, _guard, _home) = isolated_ready_repo();
        let outcome = set_semantic_triage(&ActionContext::new(repo.path()), true);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
        assert!(Config::load(repo.path()).unwrap().enable_semantic_triage);
    }

    #[test]
    #[cfg(not(feature = "semantic-triage"))]
    fn enabling_semantic_triage_requires_feature_build() {
        let (_lock, repo, _guard, _home) = isolated_ready_repo();
        let outcome = set_semantic_triage(&ActionContext::new(repo.path()), true);
        assert!(
            matches!(outcome, ActionOutcome::Error { ref message } if message.contains("optional")),
            "got {outcome:?}"
        );
        assert!(!Config::load(repo.path()).unwrap().enable_semantic_triage);
    }
}
