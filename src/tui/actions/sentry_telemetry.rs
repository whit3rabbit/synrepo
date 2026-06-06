use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use crate::config::Config;
use crate::pipeline::writer::acquire_write_admission;

use super::helpers::{load_repo_config, lock_error_to_action};
use super::{ActionContext, ActionOutcome};

/// Persist the repo-local failed-MCP-tool Sentry telemetry policy.
///
/// This writes only the boolean policy gate. The DSN must stay in the MCP
/// process environment as `SYNREPO_SENTRY_DSN`.
pub fn set_mcp_sentry_telemetry(ctx: &ActionContext, desired: bool) -> ActionOutcome {
    let config = match load_repo_config(ctx, "sentry telemetry") {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    if config.mcp_sentry_telemetry_enabled() == desired {
        return ActionOutcome::Completed {
            message: sentry_message(desired),
        };
    }

    let _lock = match acquire_write_admission(&ctx.synrepo_dir, "sentry-telemetry") {
        Ok(lock) => lock,
        Err(err) => return lock_error_to_action(&ctx.synrepo_dir, err),
    };

    let path = ctx.synrepo_dir.join("config.toml");
    match patch_mcp_sentry_telemetry(&path, desired)
        .and_then(|_| Config::load(&ctx.repo_root).map_err(anyhow::Error::from))
    {
        Ok(updated) if updated.mcp_sentry_telemetry_enabled() == desired => {
            ActionOutcome::Completed {
                message: sentry_message(desired),
            }
        }
        Ok(_) => ActionOutcome::Error {
            message: "repo config was written, but merged config did not change; check ~/.synrepo/config.toml".to_string(),
        },
        Err(err) => ActionOutcome::Error {
            message: format!("sentry telemetry config update failed: {err:#}"),
        },
    }
}

fn sentry_message(enabled: bool) -> String {
    if enabled {
        "MCP Sentry telemetry allowed; MCP processes still need SYNREPO_SENTRY_DSN to send failed-tool events".to_string()
    } else {
        "MCP Sentry telemetry disabled for this repo".to_string()
    }
}

fn patch_mcp_sentry_telemetry(path: &Path, desired: bool) -> anyhow::Result<()> {
    let mut doc = load_toml_document(path)?;
    doc.insert(
        "mcp_sentry_telemetry",
        Item::Value(TomlValue::from(desired)),
    );
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
    fn enabling_sentry_telemetry_patches_repo_config() {
        let (_lock, repo, _guard) = isolated_ready_repo();
        let outcome = set_mcp_sentry_telemetry(&ActionContext::new(repo.path()), true);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
        assert!(Config::load(repo.path())
            .unwrap()
            .mcp_sentry_telemetry_enabled());
        assert!(
            std::fs::read_to_string(repo.path().join(".synrepo/config.toml"))
                .unwrap()
                .contains("mcp_sentry_telemetry = true")
        );
    }

    #[test]
    fn local_disable_overrides_global_sentry_telemetry_opt_in() {
        let (_lock, repo, _guard) = isolated_ready_repo();
        std::fs::create_dir_all(Config::global_config_path().parent().unwrap()).unwrap();
        std::fs::write(
            Config::global_config_path(),
            "mcp_sentry_telemetry = true\n",
        )
        .unwrap();

        let outcome = set_mcp_sentry_telemetry(&ActionContext::new(repo.path()), false);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "got {outcome:?}"
        );
        assert!(!Config::load(repo.path())
            .unwrap()
            .mcp_sentry_telemetry_enabled());
        assert!(
            std::fs::read_to_string(repo.path().join(".synrepo/config.toml"))
                .unwrap()
                .contains("mcp_sentry_telemetry = false")
        );
    }
}
