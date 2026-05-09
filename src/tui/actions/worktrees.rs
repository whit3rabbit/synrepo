use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use crate::config::Config;
use crate::pipeline::writer::acquire_write_admission;

use super::helpers::{load_repo_config, lock_error_to_action};
use super::{ActionContext, ActionOutcome};

/// Persist the repo-local linked-worktree discovery flag.
///
/// This intentionally does not run reconcile. Changing root discovery can
/// add or remove a whole identity domain, so the operator chooses the
/// follow-up reconcile explicitly with `R` or the CLI.
pub fn set_worktrees_enabled(ctx: &ActionContext, desired: bool) -> ActionOutcome {
    let config = match load_repo_config(ctx, "worktrees") {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    if config.include_worktrees == desired {
        return ActionOutcome::Completed {
            message: worktrees_message(desired),
        };
    }

    let _lock = match acquire_write_admission(&ctx.synrepo_dir, "worktrees") {
        Ok(lock) => lock,
        Err(err) => return lock_error_to_action(&ctx.synrepo_dir, err),
    };

    let path = ctx.synrepo_dir.join("config.toml");
    match patch_worktrees_enabled(&path, desired)
        .and_then(|_| Config::load(&ctx.repo_root).map_err(anyhow::Error::from))
    {
        Ok(updated) if updated.include_worktrees == desired => ActionOutcome::Completed {
            message: worktrees_message(desired),
        },
        Ok(_) => ActionOutcome::Error {
            message: "repo config was written, but merged config did not change; check ~/.synrepo/config.toml".to_string(),
        },
        Err(err) => ActionOutcome::Error {
            message: format!("worktrees config update failed: {err:#}"),
        },
    }
}

fn worktrees_message(enabled: bool) -> String {
    if enabled {
        "worktree discovery enabled; press R or run `synrepo reconcile` to refresh roots"
            .to_string()
    } else {
        "worktree discovery disabled; press R or run `synrepo reconcile` to retire worktree roots"
            .to_string()
    }
}

fn patch_worktrees_enabled(path: &Path, desired: bool) -> anyhow::Result<()> {
    let mut doc = load_toml_document(path)?;
    doc.insert("include_worktrees", Item::Value(TomlValue::from(desired)));
    write_toml_document(path, &doc)
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
    ) {
        let lock =
            crate::test_support::global_test_lock(crate::config::test_home::HOME_ENV_TEST_LOCK);
        let home = tempdir().unwrap();
        let guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
        let repo = tempdir().unwrap();
        crate::bootstrap::bootstrap(repo.path(), None, false).expect("bootstrap");
        (lock, repo, guard)
    }

    #[test]
    fn disabling_worktrees_patches_repo_config() {
        let (_lock, repo, _guard) = isolated_ready_repo();
        let outcome = set_worktrees_enabled(&ActionContext::new(repo.path()), false);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
        assert!(!Config::load(repo.path()).unwrap().include_worktrees);
        assert!(
            std::fs::read_to_string(repo.path().join(".synrepo/config.toml"))
                .unwrap()
                .contains("include_worktrees = false")
        );
    }

    #[test]
    fn enabling_worktrees_removes_local_opt_out_effectively() {
        let (_lock, repo, _guard) = isolated_ready_repo();
        let path = repo.path().join(".synrepo/config.toml");
        let mut config = Config::load(repo.path()).unwrap();
        config.include_worktrees = false;
        std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

        let outcome = set_worktrees_enabled(&ActionContext::new(repo.path()), true);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
        assert!(Config::load(repo.path()).unwrap().include_worktrees);
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("include_worktrees = true"));
    }
}
