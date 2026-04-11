//! synrepo MCP server — exposes the five core tools to coding agents.
//!
//! Runs over stdio (the default MCP transport). Start it with:
//!   `synrepo-mcp [--repo <path>]`
//!
//! The server opens the graph store from `.synrepo/graph/` at the repo root
//! and serves tools until the client disconnects.

use std::{path::PathBuf, sync::Arc};

use anyhow::Context as _;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt as _,
};
use serde::Deserialize;
use serde_json::json;
use synrepo::{
    config::Config,
    core::ids::NodeId,
    store::sqlite::SqliteGraphStore,
    structure::graph::EdgeKind,
    surface::card::{compiler::GraphCardCompiler, Budget, CardCompiler},
};

// ---------------------------------------------------------------------------
// Server state
// ---------------------------------------------------------------------------

/// Shared read-only state held across all tool invocations.
struct SynrepoState {
    compiler: GraphCardCompiler,
    config: Config,
    repo_root: PathBuf,
}

// SAFETY: SqliteGraphStore holds a Mutex<Connection>; Connection is Send.
unsafe impl Send for SynrepoState {}
unsafe impl Sync for SynrepoState {}

// ---------------------------------------------------------------------------
// Tool parameter types
// ---------------------------------------------------------------------------

/// Parameters for the `synrepo_card` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CardParams {
    /// Target to look up. Accepts a file path, a qualified symbol name, or a
    /// short symbol name (display name). First match wins.
    pub target: String,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

fn default_budget() -> String {
    "tiny".to_string()
}

/// Parameters for the `synrepo_search` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Lexical query string.
    pub query: String,
    /// Maximum number of results to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

/// Parameters for the `synrepo_where_to_edit` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WhereToEditParams {
    /// Plain-language description of the task (e.g. "add retry logic to HTTP client").
    pub task: String,
    /// Maximum number of file suggestions to return. Defaults to 5.
    #[serde(default = "default_edit_limit")]
    pub limit: u32,
}

fn default_edit_limit() -> u32 {
    5
}

/// Parameters for the `synrepo_change_impact` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ChangeImpactParams {
    /// Target file path or symbol name to assess change impact for.
    pub target: String,
}

// ---------------------------------------------------------------------------
// Server implementation
// ---------------------------------------------------------------------------

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
             synrepo_where_to_edit to route a task, and synrepo_change_impact \
             before large refactors.",
        )
    }
}

#[tool_router]
impl SynrepoServer {
    /// Return a structured card describing a file or symbol.
    ///
    /// Budget tiers:
    ///   tiny  (~200 tokens) — name, location, top imports/callers
    ///   normal (~500 tokens) — full symbol list, all imports
    ///   deep  (~2k tokens)  — normal + full source body
    #[tool(
        name = "synrepo_card",
        description = "Return a structured card describing a file or symbol."
    )]
    async fn synrepo_card(
        &self,
        Parameters(CardParams { target, budget }): Parameters<CardParams>,
    ) -> String {
        let budget = parse_budget(&budget);
        let result: anyhow::Result<serde_json::Value> = (|| {
            let node_id = self
                .state
                .compiler
                .resolve_target(&target)?
                .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

            match node_id {
                NodeId::Symbol(sym_id) => {
                    let card = self.state.compiler.symbol_card(sym_id, budget)?;
                    Ok(serde_json::to_value(&card)?)
                }
                NodeId::File(file_id) => {
                    let card = self.state.compiler.file_card(file_id, budget)?;
                    Ok(serde_json::to_value(&card)?)
                }
                NodeId::Concept(concept_id) => {
                    let concept = self
                        .state
                        .compiler
                        .graph()
                        .get_concept(concept_id)?
                        .ok_or_else(|| anyhow::anyhow!("concept not found"))?;
                    Ok(serde_json::to_value(&concept)?)
                }
            }
        })();
        render_result(result)
    }

    /// Search the repository using lexical queries.
    #[tool(
        name = "synrepo_search",
        description = "Search the repository using lexical queries."
    )]
    async fn synrepo_search(
        &self,
        Parameters(SearchParams { query, limit }): Parameters<SearchParams>,
    ) -> String {
        let result: anyhow::Result<serde_json::Value> = (|| {
            let matches =
                synrepo::substrate::search(&self.state.config, &self.state.repo_root, &query)?;

            let items: Vec<serde_json::Value> = matches
                .into_iter()
                .take(limit as usize)
                .map(|m| {
                    json!({
                        "path": m.path.to_string_lossy(),
                        "line": m.line_number,
                        "content": String::from_utf8_lossy(&m.line_content).trim_end().to_string(),
                    })
                })
                .collect();

            Ok(json!({ "query": query, "results": items }))
        })();
        render_result(result)
    }

    /// Return a high-level overview of the repository's graph state.
    #[tool(
        name = "synrepo_overview",
        description = "Return a high-level overview of the repository graph state."
    )]
    async fn synrepo_overview(&self) -> String {
        let result: anyhow::Result<serde_json::Value> = (|| {
            let synrepo_dir = Config::synrepo_dir(&self.state.repo_root);
            let graph_dir = synrepo_dir.join("graph");
            let store = SqliteGraphStore::open_existing(&graph_dir)?;
            let stats = store.persisted_stats()?;
            Ok(json!({
                "mode": self.state.config.mode.to_string(),
                "graph": {
                    "file_nodes": stats.file_nodes,
                    "symbol_nodes": stats.symbol_nodes,
                    "concept_nodes": stats.concept_nodes,
                    "total_edges": stats.total_edges,
                    "edges_by_kind": stats.edge_counts_by_kind,
                }
            }))
        })();
        render_result(result)
    }

    /// Suggest where to make edits for a plain-language task description.
    #[tool(
        name = "synrepo_where_to_edit",
        description = "Suggest where to make edits for a plain-language task description."
    )]
    async fn synrepo_where_to_edit(
        &self,
        Parameters(WhereToEditParams { task, limit }): Parameters<WhereToEditParams>,
    ) -> String {
        let result: anyhow::Result<serde_json::Value> = (|| {
            let matches =
                synrepo::substrate::search(&self.state.config, &self.state.repo_root, &task)?;

            // Group results by file path, taking the top `limit` unique files.
            let mut seen = std::collections::HashSet::new();
            let mut cards = Vec::new();

            for m in &matches {
                let path = m.path.to_string_lossy().to_string();
                if seen.contains(&path) {
                    continue;
                }
                seen.insert(path.clone());

                if let Some(file) = self.state.compiler.graph().file_by_path(&path)? {
                    let card = self.state.compiler.file_card(file.id, Budget::Tiny)?;
                    cards.push(serde_json::to_value(&card)?);
                }

                if cards.len() >= limit as usize {
                    break;
                }
            }

            Ok(json!({ "task": task, "suggestions": cards }))
        })();
        render_result(result)
    }

    /// Assess the change impact of modifying a file or symbol.
    #[tool(
        name = "synrepo_change_impact",
        description = "Assess the change impact of modifying a file or symbol."
    )]
    async fn synrepo_change_impact(
        &self,
        Parameters(ChangeImpactParams { target }): Parameters<ChangeImpactParams>,
    ) -> String {
        let result: anyhow::Result<serde_json::Value> = (|| {
            let node_id = self
                .state
                .compiler
                .resolve_target(&target)?
                .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

            // Collect inbound Imports and Calls edges.
            let imports_in = self
                .state
                .compiler
                .graph()
                .inbound(node_id, Some(EdgeKind::Imports))?;
            let calls_in = self
                .state
                .compiler
                .graph()
                .inbound(node_id, Some(EdgeKind::Calls))?;

            let mut impacted_files: Vec<serde_json::Value> = Vec::new();
            let mut seen_files = std::collections::HashSet::new();

            for edge in imports_in.iter().chain(calls_in.iter()) {
                let file_id = match edge.from {
                    NodeId::File(id) => id,
                    NodeId::Symbol(sym_id) => {
                        // Get the file that owns this symbol.
                        if let Some(sym) = self.state.compiler.graph().get_symbol(sym_id)? {
                            sym.file_id
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };

                if seen_files.insert(file_id) {
                    if let Some(file) = self.state.compiler.graph().get_file(file_id)? {
                        impacted_files.push(json!({
                            "path": file.path,
                            "edge_kind": edge.kind.as_str(),
                        }));
                    }
                }
            }

            Ok(json!({
                "target": target,
                "impacted_files": impacted_files,
                "total": impacted_files.len(),
            }))
        })();
        render_result(result)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_budget(s: &str) -> Budget {
    match s {
        "normal" => Budget::Normal,
        "deep" => Budget::Deep,
        _ => Budget::Tiny,
    }
}

fn render_result(result: anyhow::Result<serde_json::Value>) -> String {
    match result {
        Ok(val) => serde_json::to_string_pretty(&val)
            .unwrap_or_else(|e| json!({ "error": e.to_string() }).to_string()),
        Err(err) => serde_json::to_string_pretty(&json!({ "error": err.to_string() }))
            .unwrap_or_else(|_| r#"{"error":"serialization failure"}"#.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Simple --repo <path> argument, defaulting to cwd.
    let repo_root = {
        let args: Vec<String> = std::env::args().collect();
        let repo_idx = args.iter().position(|a| a == "--repo");
        if let Some(idx) = repo_idx {
            args.get(idx + 1)
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().expect("cwd"))
        } else {
            std::env::current_dir().expect("cwd")
        }
    };

    let config = Config::load(&repo_root).with_context(|| {
        format!(
            "Could not load config from {}/.synrepo/config.toml — run `synrepo init` first",
            repo_root.display()
        )
    })?;

    let synrepo_dir = Config::synrepo_dir(&repo_root);
    let graph_dir = synrepo_dir.join("graph");
    let graph = SqliteGraphStore::open_existing(&graph_dir).with_context(|| {
        format!(
            "Graph store not found at {} — run `synrepo init` first",
            graph_dir.display()
        )
    })?;

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo_root.clone()));

    let state = SynrepoState {
        compiler,
        config,
        repo_root,
    };

    let server = SynrepoServer::new(state);
    let transport = stdio();
    server.serve(transport).await?;
    Ok(())
}
