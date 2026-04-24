use super::*;
use crate::tui::theme::{Theme, ThemeVariant};

#[test]
fn tui_options_default_has_color_on() {
    let opts = TuiOptions::default();
    assert!(!opts.no_color);
}

#[test]
fn no_color_flag_maps_to_plain_theme() {
    // --no-color still enters the TUI but uses Theme::plain() so no ANSI
    // codes are emitted. The theme construction is pure, so we pin the
    // mapping here without needing to actually drive a PTY.
    let theme = Theme::from_no_color(true);
    assert_eq!(theme.variant, ThemeVariant::Plain);
}

#[test]
fn color_on_maps_to_dark_theme() {
    let theme = Theme::from_no_color(false);
    assert_eq!(theme.variant, ThemeVariant::Dark);
}

#[test]
fn dashboard_options_from_tui_options_propagates_no_color() {
    let tui = TuiOptions { no_color: true };
    let dash: DashboardOptions = tui.into();
    assert!(dash.no_color);
    assert!(
        !dash.welcome_banner,
        "welcome_banner defaults to false when converting from TuiOptions",
    );
}

#[test]
fn dashboard_options_default_has_color_on_and_no_banner() {
    let opts = DashboardOptions::default();
    assert!(!opts.no_color);
    assert!(!opts.welcome_banner);
}

/// Pin the pipe-out path with a test-only stdout override. The Rust test
/// harness may still attach the process stdout to a real terminal, so these
/// tests must not depend on how the runner was launched.
#[test]
fn pipe_out_run_dashboard_returns_non_tty_fallback() {
    use crate::bootstrap::runtime_probe::AgentIntegration;
    let tempdir = tempfile::tempdir().unwrap();
    let _stdout = force_stdout_is_tty_for_test(false);
    assert!(
        !stdout_is_tty(),
        "test override should simulate piped stdout"
    );
    let outcome = run_dashboard(
        tempdir.path(),
        AgentIntegration::Absent,
        TuiOptions::default(),
    )
    .expect("short-circuit is infallible");
    assert_eq!(outcome, TuiOutcome::NonTtyFallback);
}

#[test]
fn pipe_out_run_setup_wizard_returns_non_tty() {
    let tempdir = tempfile::tempdir().unwrap();
    let _stdout = force_stdout_is_tty_for_test(false);
    assert!(
        !stdout_is_tty(),
        "test override should simulate piped stdout"
    );
    let outcome = run_setup_wizard(tempdir.path(), TuiOptions::default())
        .expect("short-circuit is infallible");
    assert_eq!(outcome, SetupWizardOutcome::NonTty);
}

#[test]
fn pipe_out_run_repair_wizard_returns_non_tty() {
    let tempdir = tempfile::tempdir().unwrap();
    let _stdout = force_stdout_is_tty_for_test(false);
    assert!(
        !stdout_is_tty(),
        "test override should simulate piped stdout"
    );
    let outcome = run_repair_wizard(tempdir.path(), Vec::new(), TuiOptions::default())
        .expect("short-circuit is infallible");
    assert_eq!(outcome, RepairWizardOutcome::NonTty);
}

#[test]
fn pipe_out_run_integration_wizard_returns_non_tty() {
    use crate::bootstrap::runtime_probe::AgentIntegration;
    let tempdir = tempfile::tempdir().unwrap();
    let _stdout = force_stdout_is_tty_for_test(false);
    assert!(
        !stdout_is_tty(),
        "test override should simulate piped stdout"
    );
    let outcome = run_integration_wizard(
        tempdir.path(),
        AgentIntegration::Absent,
        TuiOptions::default(),
    )
    .expect("short-circuit is infallible");
    assert_eq!(outcome, IntegrationWizardOutcome::NonTty);
}

#[test]
fn ensure_watch_daemon_starts_watch_for_ready_repo_without_log_noise() {
    let _guard = crate::test_support::global_test_lock("tui-ensure-watch-daemon");
    let home = tempfile::tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let tempdir = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");

    let logs = ensure_watch_daemon_for_dashboard(tempdir.path());
    assert!(
        logs.is_empty(),
        "successful auto-start should not seed an error log: {logs:?}"
    );
    let ctx = crate::tui::actions::ActionContext::new(tempdir.path());
    assert!(
        matches!(
            watch_service_status(&ctx.synrepo_dir),
            WatchServiceStatus::Running(_)
        ),
        "watch should be running after auto-start"
    );

    let stop = crate::tui::actions::stop_watch(&ctx);
    assert!(
        matches!(
            stop,
            crate::tui::actions::ActionOutcome::Ack { .. }
                | crate::tui::actions::ActionOutcome::Completed { .. }
        ),
        "cleanup stop must succeed, got {stop:?}"
    );
}

#[test]
fn ensure_watch_daemon_preserves_existing_running_service() {
    let _guard = crate::test_support::global_test_lock("tui-ensure-watch-daemon");
    let home = tempfile::tempdir().unwrap();
    let _home_guard = crate::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let tempdir = tempfile::tempdir().unwrap();
    crate::bootstrap::bootstrap(tempdir.path(), None, false).expect("bootstrap");
    let ctx = crate::tui::actions::ActionContext::new(tempdir.path());
    let start = crate::tui::actions::start_watch_daemon(&ctx);
    assert!(
        matches!(start, crate::tui::actions::ActionOutcome::Ack { .. }),
        "setup start must succeed, got {start:?}"
    );

    let before_pid = match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => state.pid,
        other => panic!("expected running watch before second ensure, got {other:?}"),
    };
    let logs = ensure_watch_daemon_for_dashboard(tempdir.path());
    assert!(
        logs.is_empty(),
        "existing watch should not emit startup logs"
    );
    let after_pid = match watch_service_status(&ctx.synrepo_dir) {
        WatchServiceStatus::Running(state) => state.pid,
        other => panic!("expected running watch after second ensure, got {other:?}"),
    };
    assert_eq!(
        before_pid, after_pid,
        "ensure must not replace the running daemon"
    );

    let stop = crate::tui::actions::stop_watch(&ctx);
    assert!(
        matches!(
            stop,
            crate::tui::actions::ActionOutcome::Ack { .. }
                | crate::tui::actions::ActionOutcome::Completed { .. }
        ),
        "cleanup stop must succeed, got {stop:?}"
    );
}

#[test]
fn ensure_watch_daemon_returns_blocked_startup_log_on_failure() {
    let tempdir = tempfile::tempdir().unwrap();
    let logs = ensure_watch_daemon_for_dashboard(tempdir.path());
    assert_eq!(
        logs.len(),
        1,
        "failed auto-start should seed one startup log"
    );
    let entry = &logs[0];
    assert_eq!(entry.tag, "watch");
    assert!(matches!(
        entry.severity,
        crate::tui::probe::Severity::Blocked
    ));
    assert!(
        entry.message.contains("not initialized"),
        "startup log should explain the failure: {:?}",
        entry.message
    );
}
