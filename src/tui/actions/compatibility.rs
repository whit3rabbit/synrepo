use crate::{
    pipeline::{
        maintenance::apply_compatibility_report, watch::ReconcileOutcome,
        writer::acquire_write_admission,
    },
    store::compatibility::{evaluate_runtime, CompatAction},
};

use super::{
    helpers::{load_repo_config, lock_error_to_action},
    ActionContext, ActionOutcome,
};

/// Apply pending non-blocking storage compatibility actions.
pub fn apply_compatibility_now(ctx: &ActionContext) -> ActionOutcome {
    let config = match load_repo_config(ctx, "compatibility") {
        Ok(config) => config,
        Err(outcome) => return outcome,
    };

    let report = match evaluate_runtime(&ctx.synrepo_dir, ctx.synrepo_dir.exists(), &config) {
        Ok(report) => report,
        Err(err) => {
            return ActionOutcome::Error {
                message: format!("compatibility: evaluation failed: {err}"),
            };
        }
    };

    if let Some(entry) = report
        .entries
        .iter()
        .find(|entry| entry.action == CompatAction::Block)
    {
        return ActionOutcome::Error {
            message: format!(
                "compatibility blocked: {} requires manual intervention ({})",
                entry.store_id.as_str(),
                entry.reason
            ),
        };
    }

    let has_work = report
        .entries
        .iter()
        .any(|entry| entry.action != CompatAction::Continue);
    if !has_work {
        return ActionOutcome::Completed {
            message: "compatibility already current".to_string(),
        };
    }

    let lock = match acquire_write_admission(&ctx.synrepo_dir, "compatibility") {
        Ok(lock) => lock,
        Err(err) => return lock_error_to_action(&ctx.synrepo_dir, err),
    };

    let summary =
        match apply_compatibility_report(&ctx.repo_root, &config, &ctx.synrepo_dir, &report, &lock)
        {
            Ok(summary) => summary,
            Err(err) => {
                return ActionOutcome::Error {
                    message: format!("compatibility apply failed: {err}"),
                };
            }
        };

    match summary.reconcile_outcome {
        Some(ReconcileOutcome::Completed(reconcile)) => ActionOutcome::Completed {
            message: format!(
                "compatibility applied ({} stores); reconcile completed ({} files, {} symbols)",
                summary.applied.len(),
                reconcile.files_discovered,
                reconcile.symbols_extracted
            ),
        },
        Some(ReconcileOutcome::Failed(message)) => ActionOutcome::Error {
            message: format!("compatibility apply incomplete: reconcile failed: {message}"),
        },
        Some(ReconcileOutcome::LockConflict { holder_pid }) => ActionOutcome::Conflict {
            owner_pid: Some(holder_pid),
            acquired_at: None,
            surface: "writer lock".to_string(),
            guidance: format!(
                "compatibility reconcile skipped: writer lock held by pid {holder_pid}"
            ),
        },
        None => ActionOutcome::Completed {
            message: format!("compatibility applied ({} stores)", summary.applied.len()),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::{
        config::{test_home::HomeEnvGuard, Config},
        pipeline::watch::load_reconcile_state,
        store::compatibility::evaluate_runtime,
    };

    fn bootstrapped_repo() -> (TempDir, TempDir, HomeEnvGuard) {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let guard = HomeEnvGuard::redirect_to(home.path());
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(repo.path().join("src/lib.rs"), "pub fn demo() {}\n").unwrap();
        crate::bootstrap::bootstrap(repo.path(), None, false).expect("bootstrap");
        (repo, home, guard)
    }

    fn write_index_sensitive_drift(repo: &std::path::Path) {
        let synrepo_dir = Config::synrepo_dir(repo);
        let updated = Config {
            roots: vec!["src".to_string()],
            ..Config::load(repo).unwrap()
        };
        fs::write(
            synrepo_dir.join("config.toml"),
            toml::to_string_pretty(&updated).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn apply_compatibility_now_rebuilds_and_clears_guidance() {
        let (repo, _home, _guard) = bootstrapped_repo();
        write_index_sensitive_drift(repo.path());

        let ctx = ActionContext::new(repo.path());
        let config = Config::load(repo.path()).unwrap();
        let before = evaluate_runtime(&ctx.synrepo_dir, true, &config)
            .unwrap()
            .guidance_lines();
        assert!(
            before.iter().any(|line| line.contains("needs rebuild")),
            "fixture should start with rebuild guidance: {before:?}"
        );

        let outcome = apply_compatibility_now(&ctx);
        assert!(
            matches!(outcome, ActionOutcome::Completed { .. }),
            "expected completed action, got {outcome:?}"
        );

        let config = Config::load(repo.path()).unwrap();
        let after = evaluate_runtime(&ctx.synrepo_dir, true, &config)
            .unwrap()
            .guidance_lines();
        assert!(
            after.is_empty(),
            "successful compatibility apply must clear guidance: {after:?}"
        );

        let state = load_reconcile_state(&ctx.synrepo_dir)
            .expect("reconcile state must exist after rebuild-triggered apply");
        assert_eq!(state.last_outcome, "completed");
    }
}
