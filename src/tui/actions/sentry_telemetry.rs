use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use crate::config::Config;
use crate::pipeline::watch::{watch_service_status, WatchServiceStatus};
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

    // Check if watch is running/starting.
    let watch_status = watch_service_status(&ctx.synrepo_dir);
    let was_running = matches!(
        watch_status,
        WatchServiceStatus::Running(_) | WatchServiceStatus::Starting
    );

    if was_running {
        match super::stop_watch(ctx) {
            ActionOutcome::Error { message } => {
                return ActionOutcome::Error {
                    message: format!(
                        "failed to stop active watch service before updating config: {message}"
                    ),
                };
            }
            ActionOutcome::Conflict { guidance, .. } => {
                return ActionOutcome::Error {
                    message: format!("cannot stop active watch service: {guidance}"),
                };
            }
            _ => {}
        }
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
            drop(_lock); // Release the lock before attempting to restart watch daemon.

            let mut message = sentry_message(desired);
            if was_running {
                match super::start_watch_daemon(ctx) {
                    ActionOutcome::Ack { message: start_msg } | ActionOutcome::Completed { message: start_msg } => {
                        message = format!("{message}; restarted watch daemon ({start_msg})");
                    }
                    ActionOutcome::Error { message: start_err } => {
                        message = format!("{message}; failed to restart watch daemon: {start_err}");
                    }
                    ActionOutcome::Conflict { guidance, .. } => {
                        message = format!("{message}; could not restart watch daemon: {guidance}");
                    }
                }
            }
            ActionOutcome::Completed { message }
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
        "MCP Sentry telemetry allowed; MCP processes will send failed-tool events to the built-in Sentry project unless SYNREPO_SENTRY_DSN overrides it".to_string()
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

    #[test]
    fn enabling_sentry_telemetry_reports_builtin_fallback_and_env_override() {
        let (_lock, repo, _guard) = isolated_ready_repo();

        let outcome = set_mcp_sentry_telemetry(&ActionContext::new(repo.path()), true);
        if let ActionOutcome::Completed { message } = outcome {
            assert!(message.contains("MCP Sentry telemetry allowed"));
            assert!(message.contains("built-in Sentry project"));
            assert!(message.contains("SYNREPO_SENTRY_DSN overrides it"));
        } else {
            panic!("expected Completed outcome, got {:?}", outcome);
        }
    }

    #[test]
    fn enabling_sentry_telemetry_while_watch_running_restarts_watch() {
        let (_lock, repo, _guard) = isolated_ready_repo();
        let ctx = ActionContext::new(repo.path());

        // Start the watch daemon first
        let start_outcome = crate::tui::actions::start_watch_daemon(&ctx);
        assert!(
            matches!(start_outcome, ActionOutcome::Ack { .. }),
            "failed to start watch: {:?}",
            start_outcome
        );

        // Confirm watch is running
        assert!(matches!(
            crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir),
            crate::pipeline::watch::WatchServiceStatus::Running(_)
        ));

        // Now toggle telemetry
        let outcome = set_mcp_sentry_telemetry(&ctx, true);
        if let ActionOutcome::Completed { message } = outcome {
            assert!(
                message.contains("restarted watch daemon"),
                "message was: {}",
                message
            );
        } else {
            panic!("expected Completed, got {:?}", outcome);
        }

        // Confirm watch is still running
        assert!(matches!(
            crate::pipeline::watch::watch_service_status(&ctx.synrepo_dir),
            crate::pipeline::watch::WatchServiceStatus::Running(_)
        ));

        // Clean up watch
        let stop_outcome = crate::tui::actions::stop_watch(&ctx);
        assert!(matches!(
            stop_outcome,
            ActionOutcome::Ack { .. } | ActionOutcome::Completed { .. }
        ));
    }
}
