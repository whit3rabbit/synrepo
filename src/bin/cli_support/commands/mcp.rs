//! `synrepo mcp` subcommand — starts the MCP server over stdio.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt as _,
};
use synrepo::config::Config;
use synrepo::pipeline::synthesis::telemetry;
use synrepo::store::compatibility::StoreId;
use synrepo::surface::handoffs::HandoffsRequest;
use synrepo::surface::handoffs::{collect_handoffs, to_json as handoffs_to_json};
use synrepo::surface::mcp::{audit, cards, docs, primitives, search, SynrepoState};

use super::super::graph::check_store_ready;

use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for the `synrepo_next_actions` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct NextActionsParams {
    /// Maximum number of items to return. Defaults to 20.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Only include items from the last N days. Defaults to 30.
    #[serde(default)]
    pub since_days: Option<u32>,
}

#[derive(Clone)]
struct SynrepoServer {
    state: Arc<SynrepoState>,
    tool_router: ToolRouter<Self>,
}

impl SynrepoServer {
    fn new(state: SynrepoState) -> Self {
        Self {
            state: Arc::new(state),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SynrepoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "synrepo provides structured code-intelligence context. \
             Use synrepo_overview to start, synrepo_card for details, \
             synrepo_where_to_edit to route a task, synrepo_change_impact \
             before large refactors, synrepo_entrypoints to find execution \
             roots (binaries, CLI commands, HTTP handlers, library entry points), \
             synrepo_module_card for directory-level summaries, \
             synrepo_public_api for exported symbols and public API surface of a directory, \
             synrepo_change_risk to assess change risk from drift/co-change/hotspot signals, \
             synrepo_findings to see machine-authored relationship candidates, \
             synrepo_recent_activity for bounded operational event history, \
             synrepo_refresh_commentary to generate or update LLM commentary for a symbol, \
             synrepo_docs_search to search advisory synthesized commentary docs. \
             Low-level primitives: synrepo_node, synrepo_edges, synrepo_query, \
             synrepo_overlay, synrepo_provenance for direct graph and overlay access.",
        )
    }
}

#[tool_router]
impl SynrepoServer {
    #[tool(
        name = "synrepo_card",
        description = "Return a structured card describing a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_card(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        cards::handle_card(&self.state, params.target, params.budget)
    }

    #[tool(
        name = "synrepo_search",
        description = "Search the repository using lexical queries."
    )]
    async fn synrepo_search(&self, Parameters(params): Parameters<search::SearchParams>) -> String {
        search::handle_search(&self.state, params.query, params.limit)
    }

    #[tool(
        name = "synrepo_docs_search",
        description = "Search advisory synthesized commentary docs materialized under .synrepo/. Results are overlay-backed, freshness-labeled, and never canonical graph facts."
    )]
    async fn synrepo_docs_search(
        &self,
        Parameters(params): Parameters<docs::DocsSearchParams>,
    ) -> String {
        docs::handle_docs_search(&self.state, params.query, params.limit)
    }

    #[tool(
        name = "synrepo_overview",
        description = "Return a high-level overview of the repository graph state."
    )]
    async fn synrepo_overview(&self) -> String {
        search::handle_overview(&self.state)
    }

    #[tool(
        name = "synrepo_node",
        description = "Look up a graph node by display ID. Returns full stored metadata as JSON."
    )]
    async fn synrepo_node(&self, Parameters(params): Parameters<primitives::NodeParams>) -> String {
        primitives::handle_node(&self.state, params.id)
    }

    #[tool(
        name = "synrepo_edges",
        description = "Traverse edges from a node. Optional direction (outbound/inbound) and edge type filters."
    )]
    async fn synrepo_edges(
        &self,
        Parameters(params): Parameters<primitives::EdgesParams>,
    ) -> String {
        primitives::handle_edges(&self.state, params.id, params.direction, params.edge_types)
    }

    #[tool(
        name = "synrepo_query",
        description = "Structured graph query: 'outbound <node_id> [edge_kind]' or 'inbound <node_id> [edge_kind]'."
    )]
    async fn synrepo_query(
        &self,
        Parameters(params): Parameters<primitives::QueryParams>,
    ) -> String {
        primitives::handle_query(&self.state, params.query)
    }

    #[tool(
        name = "synrepo_overlay",
        description = "Inspect overlay data for a node: commentary and proposed cross-links. Returns {overlay: null} when none exists."
    )]
    async fn synrepo_overlay(
        &self,
        Parameters(params): Parameters<primitives::OverlayParams>,
    ) -> String {
        primitives::handle_overlay(&self.state, params.id)
    }

    #[tool(
        name = "synrepo_provenance",
        description = "Audit provenance for a node and its incident edges: source, created_by, source_ref for each."
    )]
    async fn synrepo_provenance(
        &self,
        Parameters(params): Parameters<primitives::ProvenanceParams>,
    ) -> String {
        primitives::handle_provenance(&self.state, params.id)
    }

    #[tool(
        name = "synrepo_where_to_edit",
        description = "Suggest where to make edits for a plain-language task description. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_where_to_edit(
        &self,
        Parameters(params): Parameters<search::WhereToEditParams>,
    ) -> String {
        search::handle_where_to_edit(&self.state, params.task, params.limit)
    }

    #[tool(
        name = "synrepo_change_impact",
        description = "Assess the change impact of modifying a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_change_impact(
        &self,
        Parameters(params): Parameters<search::ChangeImpactParams>,
    ) -> String {
        search::handle_change_impact(&self.state, params.target)
    }

    #[tool(
        name = "synrepo_entrypoints",
        description = "Return detected execution entry points (binaries, CLI commands, HTTP handlers, library roots) for an optional path-prefix scope. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_entrypoints(
        &self,
        Parameters(params): Parameters<cards::EntrypointsParams>,
    ) -> String {
        cards::handle_entrypoints(&self.state, params.scope, params.budget)
    }

    #[tool(
        name = "synrepo_module_card",
        description = "Return a ModuleCard summarizing a directory: files, nested modules, public symbols, and token budget. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_module_card(
        &self,
        Parameters(params): Parameters<cards::ModuleCardParams>,
    ) -> String {
        cards::handle_module_card(&self.state, params.path, params.budget)
    }

    #[tool(
        name = "synrepo_public_api",
        description = "Return a PublicAPICard for a directory: public symbols with kinds and signatures, public entry points, and (at deep budget) recently changed public API surface. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_public_api(
        &self,
        Parameters(params): Parameters<cards::PublicAPICardParams>,
    ) -> String {
        cards::handle_public_api(&self.state, params.path, params.budget)
    }

    #[tool(
        name = "synrepo_minimum_context",
        description = "Return the minimum-useful context neighborhood for a symbol or file: focal card, outbound structural neighbors, governing decisions, and co-change partners. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_minimum_context(
        &self,
        Parameters(params): Parameters<cards::MinimumContextParams>,
    ) -> String {
        cards::handle_minimum_context(&self.state, params.target, params.budget)
    }

    #[tool(
        name = "synrepo_call_path",
        description = "Return a CallPathCard tracing execution paths from entry points to a target symbol using backward BFS over Calls edges. Use to understand how to reach a function from binary/CLI/HTTP entry points. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_call_path(
        &self,
        Parameters(params): Parameters<cards::CallPathParams>,
    ) -> String {
        cards::handle_call_path(&self.state, params.target, params.budget)
    }

    #[tool(
        name = "synrepo_test_surface",
        description = "Return a TestSurfaceCard discovering test functions related to a file or directory scope (beta fidelity). Uses path-convention heuristics to associate test files with source files. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_test_surface(
        &self,
        Parameters(params): Parameters<cards::TestSurfaceParams>,
    ) -> String {
        cards::handle_test_surface(&self.state, params.scope, params.budget)
    }

    #[tool(
        name = "synrepo_change_risk",
        description = "Return a change risk assessment for a symbol or file (beta fidelity), aggregating drift score, co-change partners, and git hotspot data."
    )]
    async fn synrepo_change_risk(
        &self,
        Parameters(params): Parameters<cards::ChangeRiskParams>,
    ) -> String {
        cards::handle_change_risk(&self.state, params.target, params.budget)
    }

    #[tool(
        name = "synrepo_refresh_commentary",
        description = "Explicitly generate or refresh LLM-authored commentary for a symbol. Use when synrepo_card reports commentary_state: 'missing' or 'stale' and fresh prose is required."
    )]
    async fn synrepo_refresh_commentary(
        &self,
        Parameters(params): Parameters<cards::RefreshCommentaryParams>,
    ) -> String {
        cards::handle_refresh_commentary(&self.state, params.target)
    }

    #[tool(
        name = "synrepo_findings",
        description = "List operator-facing cross-link findings with provenance, tier, score, freshness, and endpoint IDs."
    )]
    async fn synrepo_findings(
        &self,
        Parameters(params): Parameters<audit::FindingsParams>,
    ) -> String {
        audit::handle_findings(
            &self.state.repo_root,
            params.node_id,
            params.kind,
            params.freshness,
            params.limit,
        )
    }

    #[tool(
        name = "synrepo_recent_activity",
        description = "Return bounded operational activity (beta fidelity): reconcile outcomes, repair events, cross-link audit entries, commentary refreshes, and git hotspots. NOT a session-memory or agent-interaction log."
    )]
    async fn synrepo_recent_activity(
        &self,
        Parameters(params): Parameters<audit::RecentActivityParams>,
    ) -> String {
        audit::handle_recent_activity(&self.state, params.kinds, params.limit, params.since)
    }

    #[tool(
        name = "synrepo_next_actions",
        description = "Return prioritized actionable items from repair-log, cross-link candidates, and git hotspots."
    )]
    async fn synrepo_next_actions(
        &self,
        Parameters(params): Parameters<NextActionsParams>,
    ) -> String {
        let request = HandoffsRequest {
            limit: params.limit.unwrap_or(20),
            since_days: params.since_days.unwrap_or(30),
        };
        match collect_handoffs(&self.state.repo_root, &self.state.config, &request) {
            Ok(items) => handoffs_to_json(&items),
            Err(e) => serde_json::json!({
                "error": e.to_string()
            })
            .to_string(),
        }
    }
}

/// Start the MCP server over stdio for the given repository root.
///
/// This is called by `synrepo mcp [--repo <path>]`. It opens the graph and
/// overlay stores, constructs the card compiler, and serves tools until the
/// client disconnects.
pub fn run_mcp_server(repo_root: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async { serve(repo_root).await })
}

/// Load config and gate on storage compatibility. Factored out of `serve` so
/// unit tests can exercise the fail-fast path without spinning up stdio.
pub(crate) fn prepare_state(repo_root: &Path) -> anyhow::Result<SynrepoState> {
    let config = Config::load(repo_root).with_context(|| {
        format!(
            "Could not load config from {}/.synrepo/config.toml — run `synrepo init` first",
            repo_root.display()
        )
    })?;

    // Fail fast if the on-disk stores are not ready to serve. Without this
    // check the MCP server accepts clients and only surfaces compatibility
    // failures per tool call, which confuses the agent and hides the
    // canonical `synrepo upgrade` remediation path.
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
    let transport = stdio();
    server.serve(transport).await?;
    Ok(())
}
