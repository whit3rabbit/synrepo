//! synrepo MCP server — exposes the five core tools to coding agents.
//!
//! Runs over stdio (the default MCP transport). Start it with:
//!   `synrepo-mcp [--repo <path>]`
//!
//! The server opens the graph store from `.synrepo/graph/` at the repo root
//! and serves tools until the client disconnects.

mod findings;

use std::{path::PathBuf, sync::Arc};

use anyhow::Context as _;
use parking_lot::Mutex;
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
    overlay::OverlayStore,
    pipeline::synthesis::{ClaudeCommentaryGenerator, CommentaryGenerator},
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::{EdgeKind, GraphStore},
    surface::card::{
        compiler::GraphCardCompiler, Budget, CardCompiler, DecisionCard, Freshness,
    },
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

// GraphStore: Send + Sync, PathBuf: Send + Sync, Config: Send + Sync.
// SynrepoState is therefore auto-Send + Sync; no unsafe needed.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<SynrepoState>();
    }
};

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

/// Parameters for the `synrepo_entrypoints` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EntrypointsParams {
    /// Optional path prefix to scope the search (e.g. `"src/bin/"`, `"src/surface/"`).
    /// When absent, all indexed files are scanned.
    pub scope: Option<String>,
    /// Budget tier: "tiny" (default), "normal", or "deep".
    #[serde(default = "default_budget")]
    pub budget: String,
}

/// Parameters for the `synrepo_findings` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FindingsParams {
    /// Optional node ID in display form.
    pub node_id: Option<String>,
    /// Optional kind to filter by (e.g. "references", "governs").
    pub kind: Option<String>,
    /// Optional freshness state to filter by.
    pub freshness: Option<String>,
    /// Maximum number of findings to return. Defaults to 20.
    #[serde(default = "default_limit")]
    pub limit: u32,
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
             synrepo_where_to_edit to route a task, synrepo_change_impact \
             before large refactors, synrepo_entrypoints to find execution \
             roots (binaries, CLI commands, HTTP handlers, library entry points), \
             and synrepo_findings to see machine-authored relationship candidates.",
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
        let result = with_graph_snapshot(self.state.compiler.graph(), || {
            let node_id = self
                .state
                .compiler
                .resolve_target(&target)?
                .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

            match node_id {
                NodeId::Symbol(sym_id) => {
                    let card = self.state.compiler.symbol_card(sym_id, budget)?;
                    let mut json = serde_json::to_value(&card)?;
                    lift_commentary_text(&mut json);
                    attach_decision_cards(
                        &mut json,
                        NodeId::Symbol(sym_id),
                        self.state.compiler.graph(),
                        budget,
                    )?;
                    Ok(json)
                }
                NodeId::File(file_id) => {
                    let card = self.state.compiler.file_card(file_id, budget)?;
                    let mut json = serde_json::to_value(&card)?;
                    attach_decision_cards(
                        &mut json,
                        NodeId::File(file_id),
                        self.state.compiler.graph(),
                        budget,
                    )?;
                    Ok(json)
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
        });
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
        // persisted_stats issues four COUNTs plus a GROUP BY on its own
        // connection, so wrap that local store (not the compiler's graph)
        // so all counts reflect one committed epoch.
        let result: anyhow::Result<serde_json::Value> = (|| {
            let synrepo_dir = Config::synrepo_dir(&self.state.repo_root);
            let graph_dir = synrepo_dir.join("graph");
            let store = SqliteGraphStore::open_existing(&graph_dir)?;
            let stats = with_graph_snapshot(&store, || Ok(store.persisted_stats()?))?;
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

            with_graph_snapshot(self.state.compiler.graph(), || {
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
            })
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
        let result: anyhow::Result<serde_json::Value> =
            with_graph_snapshot(self.state.compiler.graph(), || {
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
            });
        render_result(result)
    }

    /// Return detected execution entry points for a scope.
    ///
    /// Budget tiers:
    ///   tiny   (~30 tokens/entry) — kind, qualified name, location only
    ///   normal (~60 tokens/entry) — above + caller count and doc comment
    ///   deep  (~150 tokens/entry) — above + full signature
    #[tool(
        name = "synrepo_entrypoints",
        description = "Return detected execution entry points (binaries, CLI commands, HTTP handlers, library roots) for an optional path-prefix scope."
    )]
    async fn synrepo_entrypoints(
        &self,
        Parameters(EntrypointsParams { scope, budget }): Parameters<EntrypointsParams>,
    ) -> String {
        let budget = parse_budget(&budget);
        let result: anyhow::Result<serde_json::Value> =
            with_graph_snapshot(self.state.compiler.graph(), || {
                let card = self
                    .state
                    .compiler
                    .entry_point_card(scope.as_deref(), budget)?;
                Ok(serde_json::to_value(&card)?)
            });
        render_result(result)
    }

    /// List machine-authored cross-link findings.
    #[tool(
        name = "synrepo_findings",
        description = "List operator-facing cross-link findings with provenance, tier, score, freshness, and endpoint IDs."
    )]
    async fn synrepo_findings(
        &self,
        Parameters(FindingsParams {
            node_id,
            kind,
            freshness,
            limit,
        }): Parameters<FindingsParams>,
    ) -> String {
        let result =
            findings::render_findings(&self.state.repo_root, node_id, kind, freshness, limit);
        render_result(result)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// If `node_id` has incoming Governs edges, build DecisionCards and attach
/// them to the JSON card object under the key `"decision_cards"`.
/// The key is absent (not null) when no governing concepts exist.
fn attach_decision_cards(
    json: &mut serde_json::Value,
    node_id: NodeId,
    graph: &dyn synrepo::structure::graph::GraphStore,
    budget: Budget,
) -> anyhow::Result<()> {
    let concepts = graph.find_governing_concepts(node_id)?;
    if concepts.is_empty() {
        return Ok(());
    }

    let mut cards: Vec<serde_json::Value> = Vec::new();
    for concept in concepts {
        // Collect all node IDs this concept governs (outbound Governs edges).
        let governs_edges = graph.outbound(NodeId::Concept(concept.id), Some(EdgeKind::Governs))?;
        let governed_node_ids: Vec<NodeId> = governs_edges.iter().map(|e| e.to).collect();

        let dc = DecisionCard {
            title: concept.title.clone(),
            status: concept.status.clone(),
            decision_body: concept.decision_body.clone(),
            governed_node_ids,
            source_path: concept.path.clone(),
            freshness: Freshness::Fresh, // git-based freshness is a later phase
        };
        cards.push(dc.render(budget));
    }

    if let serde_json::Value::Object(ref mut map) = json {
        map.insert(
            "decision_cards".to_string(),
            serde_json::Value::Array(cards),
        );
    }
    Ok(())
}

/// Mirror `overlay_commentary.text` onto a top-level `commentary_text` key
/// so MCP callers can branch on a flat field without traversing the nested
/// object. Absent when there is no commentary to surface.
fn lift_commentary_text(json: &mut serde_json::Value) {
    let Some(obj) = json.as_object_mut() else {
        return;
    };
    let text = obj
        .get("overlay_commentary")
        .and_then(|oc| oc.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    if let Some(t) = text {
        obj.insert("commentary_text".to_string(), serde_json::Value::String(t));
    }
}

/// Hold a read snapshot across the whole handler body.
///
/// The graph snapshot methods are re-entrant, so wrapping here composes
/// safely with the per-call wraps inside `GraphCardCompiler`. Any error
/// from `end_read_snapshot` is intentionally swallowed (debug-logged) so
/// the handler's original `Err` is never masked.
fn with_graph_snapshot<R>(
    graph: &dyn GraphStore,
    f: impl FnOnce() -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    graph.begin_read_snapshot()?;
    let out = f();
    // Intentionally swallow end-snapshot errors so the handler's original
    // error path is never masked. The snapshot does not outlive this frame.
    let _ = graph.end_read_snapshot();
    out
}

fn parse_budget(s: &str) -> Budget {
    match s.to_ascii_lowercase().as_str() {
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

    // Lazily-materialized overlay store for commentary. Open here (not during
    // `synrepo init`) so that `synrepo_card` at Deep budget can retrieve and
    // persist entries without a prior init-time overlay file.
    let overlay_dir = synrepo_dir.join("overlay");
    let overlay_store = SqliteOverlayStore::open(&overlay_dir)
        .with_context(|| format!("Failed to open overlay store at {}", overlay_dir.display()))?;
    let overlay: Arc<Mutex<dyn OverlayStore>> = Arc::new(Mutex::new(overlay_store));

    // Commentary generator: live Claude path if SYNREPO_ANTHROPIC_API_KEY is
    // set, otherwise a NoOp that leaves the overlay untouched.
    let boxed_generator = ClaudeCommentaryGenerator::new_or_noop(config.commentary_cost_limit);
    let generator: Arc<dyn CommentaryGenerator> = Arc::from(boxed_generator);

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo_root.clone()))
        .with_overlay(Some(overlay.clone()), Some(generator));

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
