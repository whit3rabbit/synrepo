use std::path::Path;
use std::time::Duration;

use anyhow::Context as _;
use rmcp::{transport::stdio, ServiceExt as _};
use synrepo::config::Config;
use synrepo::pipeline::explain::telemetry;
use synrepo::store::compatibility::StoreId;
use synrepo::surface::mcp::SynrepoState;

use super::super::graph::check_store_ready;
use super::mcp::SynrepoServer;

/// Start the MCP server over stdio for the given repository root.
pub(crate) fn run_mcp_server(
    repo_root: &Path,
    allow_edits: bool,
    explicit_repo: bool,
    call_timeout: &str,
) -> anyhow::Result<()> {
    let call_timeout = parse_call_timeout(call_timeout)?;
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async { serve(repo_root, allow_edits, explicit_repo, call_timeout).await })
}

/// Load config and gate on storage compatibility.
pub(crate) fn prepare_state(repo_root: &Path) -> anyhow::Result<SynrepoState> {
    let config = Config::load(repo_root).context("run `synrepo init` to initialize")?;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    check_store_ready(&synrepo_dir, &config, StoreId::Graph)?;
    check_store_ready(&synrepo_dir, &config, StoreId::Overlay)?;
    telemetry::set_synrepo_dir(&synrepo_dir);

    Ok(SynrepoState {
        config,
        repo_root: repo_root.to_path_buf(),
    })
}

async fn serve(
    repo_root: &Path,
    allow_edits: bool,
    explicit_repo: bool,
    call_timeout: Duration,
) -> anyhow::Result<()> {
    let default_state = match prepare_state(repo_root) {
        Ok(state) => Some(state),
        Err(error) if explicit_repo => return Err(error),
        Err(error) => {
            tracing::debug!(
                error = %error,
                repo_root = %repo_root.display(),
                "starting MCP server without a default repository"
            );
            None
        }
    };
    let server = SynrepoServer::new_optional_with_timeout(default_state, allow_edits, call_timeout);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn parse_call_timeout(raw: &str) -> anyhow::Result<Duration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("--call-timeout cannot be empty");
    }
    let digits = trimmed.trim_end_matches(|ch: char| ch.is_ascii_alphabetic());
    let suffix = &trimmed[digits.len()..];
    let value: u64 = digits
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid --call-timeout value: {raw}"))?;
    match suffix {
        "" | "s" => Ok(Duration::from_secs(value)),
        "ms" => Ok(Duration::from_millis(value)),
        "m" => Ok(Duration::from_secs(value.saturating_mul(60))),
        _ => anyhow::bail!("invalid --call-timeout unit: {raw}"),
    }
}
