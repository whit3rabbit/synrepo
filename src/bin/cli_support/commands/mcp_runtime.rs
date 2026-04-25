use std::path::Path;

use anyhow::Context as _;
use rmcp::{transport::stdio, ServiceExt as _};
use synrepo::config::Config;
use synrepo::pipeline::explain::telemetry;
use synrepo::store::compatibility::StoreId;
use synrepo::surface::mcp::SynrepoState;

use super::super::graph::check_store_ready;
use super::mcp::SynrepoServer;

/// Start the MCP server over stdio for the given repository root.
pub(crate) fn run_mcp_server(repo_root: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async { serve(repo_root).await })
}

/// Load config and gate on storage compatibility.
pub(crate) fn prepare_state(repo_root: &Path) -> anyhow::Result<SynrepoState> {
    let config = Config::load(repo_root).with_context(|| {
        format!(
            "Could not load config from {}/.synrepo/config.toml — run `synrepo init` first",
            repo_root.display()
        )
    })?;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    check_store_ready(&synrepo_dir, &config, StoreId::Graph)?;
    check_store_ready(&synrepo_dir, &config, StoreId::Overlay)?;
    telemetry::set_synrepo_dir(&synrepo_dir);

    Ok(SynrepoState {
        config,
        repo_root: repo_root.to_path_buf(),
    })
}

async fn serve(repo_root: &Path) -> anyhow::Result<()> {
    let state = prepare_state(repo_root)?;
    let server = SynrepoServer::new(state);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
