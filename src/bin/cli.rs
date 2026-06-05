//! synrepo CLI entry point. Bare `synrepo` routes to dashboard/setup/repair.

mod cli_support;

use clap::Parser;
use synrepo::tui::TuiOptions;
use tracing_subscriber::EnvFilter;

#[cfg(test)]
use cli_support::commands::{export, prepare_mcp_state, report_reconcile_outcome, sync, upgrade};
use cli_support::{cli_args::Cli, dispatch::dispatch, entry::run_bare_entrypoint};
// Re-exported for `cli_support::tests::agent_setup` via `crate::agent_setup`.
// cli.rs dispatches through `agent_setup_many` but the test binary compiles
// without `cfg(test)`, so this import must be unconditional.
#[allow(unused_imports)]
use cli_support::commands::agent_setup;

const DEFAULT_LOG_FILTER: &str = "warn,synrepo=info";

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER)),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let explicit_repo = cli.repo.is_some();
    let repo_root = match cli.repo {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("cannot determine working directory: {e}"))?,
    };

    let tui_opts = TuiOptions {
        no_color: cli.no_color,
    };

    match cli.command {
        None => run_bare_entrypoint(&repo_root, tui_opts, explicit_repo),
        Some(cmd) => dispatch(cmd, &repo_root, tui_opts, explicit_repo),
    }
}

#[cfg(test)]
mod logging_tests {
    use super::*;

    #[test]
    fn default_log_filter_keeps_third_party_info_quiet() {
        assert_eq!(DEFAULT_LOG_FILTER, "warn,synrepo=info");
        assert!(EnvFilter::try_new(DEFAULT_LOG_FILTER).is_ok());
    }
}
