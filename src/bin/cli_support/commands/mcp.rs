//! `synrepo mcp` subcommand — starts the MCP server over stdio.

use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        ListResourceTemplatesResult, PaginatedRequestParams, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RoleServer},
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use synrepo::surface::handoffs::HandoffsRequest;
use synrepo::surface::handoffs::{collect_handoffs, to_json as handoffs_to_json};
use synrepo::surface::mcp::{
    audit, cards, context_pack, docs, edits, notes, primitives, search, SynrepoState,
};

pub(crate) struct SynrepoServer {
    states: Arc<RwLock<HashMap<std::path::PathBuf, Arc<SynrepoState>>>>,
    auto_started_roots: Arc<RwLock<HashSet<std::path::PathBuf>>>,
    default_repo_root: std::path::PathBuf,
    tool_router: ToolRouter<Self>,
    allow_edits: bool,
}

impl Clone for SynrepoServer {
    fn clone(&self) -> Self {
        Self {
            states: Arc::clone(&self.states),
            auto_started_roots: Arc::clone(&self.auto_started_roots),
            default_repo_root: self.default_repo_root.clone(),
            tool_router: self.tool_router.clone(),
            allow_edits: self.allow_edits,
        }
    }
}

impl SynrepoServer {
    pub(crate) fn new(state: SynrepoState, allow_edits: bool) -> Self {
        let repo_root = state.repo_root.clone();
        let mut states = HashMap::new();
        states.insert(repo_root.clone(), Arc::new(state));
        let auto_started_roots = HashSet::new();
        let mut tool_router = Self::tool_router();
        if !allow_edits {
            tool_router.remove_route("synrepo_prepare_edit_context");
            tool_router.remove_route("synrepo_apply_anchor_edits");
        }
        let server = Self {
            states: Arc::new(RwLock::new(states)),
            auto_started_roots: Arc::new(RwLock::new(auto_started_roots)),
            default_repo_root: repo_root.clone(),
            tool_router,
            allow_edits,
        };

        // Auto-trigger watch daemon for the default root.
        if let Ok(Some(_)) = super::watch::maybe_spawn_watch_daemon(&repo_root) {
            server.auto_started_roots.write().insert(repo_root);
        }

        server
    }

    fn resolve_state(&self, param_root: Option<std::path::PathBuf>) -> Arc<SynrepoState> {
        let root = param_root.unwrap_or_else(|| self.default_repo_root.clone());
        {
            let read = self.states.read();
            if let Some(state) = read.get(&root) {
                return Arc::clone(state);
            }
        }

        let mut write = self.states.write();
        if let Some(state) = write.get(&root) {
            return Arc::clone(state);
        }

        match super::mcp_runtime::prepare_state(&root) {
            Ok(state) => {
                let state = Arc::new(state);
                write.insert(root.clone(), Arc::clone(&state));

                // Auto-trigger watch daemon for the newly resolved root.
                if let Ok(Some(_)) = super::watch::maybe_spawn_watch_daemon(&root) {
                    self.auto_started_roots.write().insert(root);
                }

                state
            }
            Err(_) => {
                let default_root = self.default_repo_root.clone();
                write
                    .get(&default_root)
                    .cloned()
                    .expect("default state must exist")
            }
        }
    }

    /// Stop all watch daemons that were auto-started by this server instance.
    pub(crate) fn stop_auto_started_watchers(&self) {
        let mut roots = self.auto_started_roots.write();
        for root in roots.drain() {
            let _ = super::watch::watch_stop(&root);
        }
    }

    /// Best-effort recording of a workflow alias call. Keeps
    /// `workflow_calls_total` in the context-metrics file separate from
    /// card-level counters so the two categories never collapse into a
    /// single aggregate.
    fn record_workflow(&self, tool: &str) {
        let state = self.resolve_state(None);
        let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
        synrepo::pipeline::context_metrics::record_workflow_call_best_effort(&synrepo_dir, tool);
    }

    #[cfg(test)]
    pub(crate) fn registered_tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SynrepoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "synrepo provides structured code-intelligence context. \
             Required workflow: synrepo_orient to start, synrepo_find to route a task, \
             synrepo_explain for bounded details, synrepo_impact (or its shorthand synrepo_risks) before edits, \
             synrepo_tests before claiming done, and synrepo_changed after edits. \
             Use synrepo_minimum_context as the bounded neighborhood step once a focal target is known. \
             Use synrepo_context_pack when batching several read-only context artifacts is cheaper than serial tool calls. \
             Edit tools are absent unless this server was started with synrepo mcp --allow-edits; when present, call prepare before apply. \
             Read full source files only after card routing identifies the target or when a bounded card is insufficient; \
             that is an explicit escalation, not the default first step. \
             Graph-backed structural facts are authoritative; overlay commentary and advisory notes never define source truth. \
             Existing task-first, audit, overlay, and graph primitive tools remain available.",
        )
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(
                RawResourceTemplate::new("synrepo://card/{target}", "synrepo card")
                    .with_description("Read a card-shaped JSON context artifact.")
                    .with_mime_type("application/json"),
                None,
            ),
            ResourceTemplate::new(
                RawResourceTemplate::new("synrepo://file/{path}/outline", "synrepo file outline")
                    .with_description("Read a compact file outline with symbols and hashes.")
                    .with_mime_type("application/json"),
                None,
            ),
            ResourceTemplate::new(
                RawResourceTemplate::new(
                    "synrepo://context-pack?goal={goal}",
                    "synrepo context pack",
                )
                .with_description("Read a batched read-only context pack.")
                .with_mime_type("application/json"),
                None,
            ),
        ])))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let state = self.resolve_state(None);
        let uri = request.uri;
        let result = match context_pack::read_resource(&state, &uri) {
            Ok(text) => Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text, uri,
            )
            .with_mime_type("application/json")])),
            Err(message) => Err(McpError::resource_not_found(message, None)),
        };
        std::future::ready(result)
    }
}

#[tool_router]
impl SynrepoServer {
    #[tool(
        name = "synrepo_card",
        description = "Return a structured card describing a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_card(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        cards::handle_card(
            &self.resolve_state(params.repo_root.clone()),
            params.target,
            params.budget,
            params.budget_tokens,
            params.include_notes,
        )
    }

    #[tool(
        name = "synrepo_search",
        description = "Search the repository using lexical queries."
    )]
    async fn synrepo_search(&self, Parameters(params): Parameters<search::SearchParams>) -> String {
        search::handle_search(
            &self.resolve_state(params.repo_root.clone()),
            params.query,
            params.limit,
        )
    }

    #[tool(
        name = "synrepo_docs_search",
        description = "Search advisory explained commentary docs materialized under .synrepo/. Results are overlay-backed, freshness-labeled, and never canonical graph facts."
    )]
    async fn synrepo_docs_search(
        &self,
        Parameters(params): Parameters<docs::DocsSearchParams>,
    ) -> String {
        docs::handle_docs_search(
            &self.resolve_state(params.repo_root.clone()),
            params.query,
            params.limit,
        )
    }

    #[tool(
        name = "synrepo_context_pack",
        description = "Batch read-only context artifacts (file outlines, cards, neighborhoods, tests, call paths, and search) into one token-accounted response. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_context_pack(
        &self,
        Parameters(params): Parameters<context_pack::ContextPackParams>,
    ) -> String {
        let repo_root = params.repo_root.clone();
        context_pack::handle_context_pack(&self.resolve_state(repo_root), params)
    }

    #[tool(
        name = "synrepo_overview",
        description = "Return a high-level overview of the repository graph state."
    )]
    async fn synrepo_overview(&self) -> String {
        search::handle_overview(&self.resolve_state(None))
    }

    #[tool(
        name = "synrepo_node",
        description = "Look up a graph node by display ID. Returns full stored metadata as JSON."
    )]
    async fn synrepo_node(&self, Parameters(params): Parameters<primitives::NodeParams>) -> String {
        primitives::handle_node(&self.resolve_state(params.repo_root.clone()), params.id)
    }

    #[tool(
        name = "synrepo_edges",
        description = "Traverse edges from a node. Optional direction (outbound/inbound) and edge type filters."
    )]
    async fn synrepo_edges(
        &self,
        Parameters(params): Parameters<primitives::EdgesParams>,
    ) -> String {
        primitives::handle_edges(
            &self.resolve_state(None),
            params.id,
            params.direction,
            params.edge_types,
        )
    }

    #[tool(
        name = "synrepo_query",
        description = "Structured graph query: 'outbound <node_id> [edge_kind]' or 'inbound <node_id> [edge_kind]'."
    )]
    async fn synrepo_query(
        &self,
        Parameters(params): Parameters<primitives::QueryParams>,
    ) -> String {
        primitives::handle_query(&self.resolve_state(None), params.query)
    }

    #[tool(
        name = "synrepo_overlay",
        description = "Inspect overlay data for a node: commentary and proposed cross-links. Returns {overlay: null} when none exists."
    )]
    async fn synrepo_overlay(
        &self,
        Parameters(params): Parameters<primitives::OverlayParams>,
    ) -> String {
        primitives::handle_overlay(&self.resolve_state(None), params.id)
    }

    #[tool(
        name = "synrepo_provenance",
        description = "Audit provenance for a node and its incident edges: source, created_by, source_ref for each."
    )]
    async fn synrepo_provenance(
        &self,
        Parameters(params): Parameters<primitives::ProvenanceParams>,
    ) -> String {
        primitives::handle_provenance(&self.resolve_state(None), params.id)
    }

    #[tool(
        name = "synrepo_where_to_edit",
        description = "Suggest where to make edits for a plain-language task description. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_where_to_edit(
        &self,
        Parameters(params): Parameters<search::WhereToEditParams>,
    ) -> String {
        search::handle_where_to_edit(
            &self.resolve_state(None),
            params.task,
            params.limit,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_change_impact",
        description = "Assess the change impact of modifying a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_change_impact(
        &self,
        Parameters(params): Parameters<search::ChangeImpactParams>,
    ) -> String {
        search::handle_change_impact(&self.resolve_state(None), params.target)
    }

    #[tool(
        name = "synrepo_entrypoints",
        description = "Return detected execution entry points (binaries, CLI commands, HTTP handlers, library roots) for an optional path-prefix scope. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_entrypoints(
        &self,
        Parameters(params): Parameters<cards::EntrypointsParams>,
    ) -> String {
        cards::handle_entrypoints(
            &self.resolve_state(None),
            params.scope,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_note_add",
        description = "Add an advisory overlay agent note. Notes are labeled source_store=overlay and advisory=true; they never define graph truth."
    )]
    async fn synrepo_note_add(
        &self,
        Parameters(params): Parameters<notes::NoteAddParams>,
    ) -> String {
        notes::handle_note_add(&self.resolve_state(None), params)
    }

    #[tool(
        name = "synrepo_note_link",
        description = "Link two advisory overlay notes while preserving audit history."
    )]
    async fn synrepo_note_link(
        &self,
        Parameters(params): Parameters<notes::NoteLinkParams>,
    ) -> String {
        notes::handle_note_link(&self.resolve_state(None), params)
    }

    #[tool(
        name = "synrepo_note_supersede",
        description = "Supersede an advisory overlay note with a replacement claim."
    )]
    async fn synrepo_note_supersede(
        &self,
        Parameters(params): Parameters<notes::NoteSupersedeParams>,
    ) -> String {
        notes::handle_note_supersede(&self.resolve_state(None), params)
    }

    #[tool(
        name = "synrepo_note_forget",
        description = "Hide an advisory overlay note from normal retrieval while retaining audit history."
    )]
    async fn synrepo_note_forget(
        &self,
        Parameters(params): Parameters<notes::NoteForgetParams>,
    ) -> String {
        notes::handle_note_forget(&self.resolve_state(None), params)
    }

    #[tool(
        name = "synrepo_note_verify",
        description = "Verify an advisory overlay note and return it to active state when anchors match."
    )]
    async fn synrepo_note_verify(
        &self,
        Parameters(params): Parameters<notes::NoteVerifyParams>,
    ) -> String {
        notes::handle_note_verify(&self.resolve_state(None), params)
    }

    #[tool(
        name = "synrepo_notes",
        description = "List bounded advisory overlay notes. Hidden lifecycle states require include_hidden=true."
    )]
    async fn synrepo_notes(&self, Parameters(params): Parameters<notes::NotesParams>) -> String {
        notes::handle_notes(&self.resolve_state(params.repo_root.clone()), params)
    }

    #[tool(
        name = "synrepo_module_card",
        description = "Return a ModuleCard summarizing a directory: files, nested modules, public symbols, and token budget. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_module_card(
        &self,
        Parameters(params): Parameters<cards::ModuleCardParams>,
    ) -> String {
        cards::handle_module_card(
            &self.resolve_state(None),
            params.path,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_public_api",
        description = "Return a PublicAPICard for a directory: public symbols with kinds and signatures, public entry points, and (at deep budget) recently changed public API surface. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_public_api(
        &self,
        Parameters(params): Parameters<cards::PublicAPICardParams>,
    ) -> String {
        cards::handle_public_api(
            &self.resolve_state(None),
            params.path,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_minimum_context",
        description = "Bounded neighborhood step for a focal symbol or file: focal card, outbound structural neighbors, governing decisions, and co-change partners. Use before deep cards or full-file reads when a target is known but surrounding risk is unclear. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_minimum_context(
        &self,
        Parameters(params): Parameters<cards::MinimumContextParams>,
    ) -> String {
        self.record_workflow("minimum_context");
        cards::handle_minimum_context(
            &self.resolve_state(None),
            params.target,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_call_path",
        description = "Return a CallPathCard tracing execution paths from entry points to a target symbol using backward BFS over Calls edges. Use to understand how to reach a function from binary/CLI/HTTP entry points. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_call_path(
        &self,
        Parameters(params): Parameters<cards::CallPathParams>,
    ) -> String {
        cards::handle_call_path(
            &self.resolve_state(None),
            params.target,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_test_surface",
        description = "Return a TestSurfaceCard discovering test functions related to a file or directory scope (beta fidelity). Uses path-convention heuristics to associate test files with source files. Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    )]
    async fn synrepo_test_surface(
        &self,
        Parameters(params): Parameters<cards::TestSurfaceParams>,
    ) -> String {
        cards::handle_test_surface(
            &self.resolve_state(None),
            params.scope,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_change_risk",
        description = "Return a change risk assessment for a symbol or file (beta fidelity), aggregating drift score, co-change partners, and git hotspot data."
    )]
    async fn synrepo_change_risk(
        &self,
        Parameters(params): Parameters<cards::ChangeRiskParams>,
    ) -> String {
        cards::handle_change_risk(
            &self.resolve_state(None),
            params.target,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_orient",
        description = "Workflow step 1: orient before reading the repo cold. Run before any cold file reads."
    )]
    async fn synrepo_orient(&self) -> String {
        self.record_workflow("orient");
        search::handle_overview(&self.resolve_state(None))
    }

    #[tool(
        name = "synrepo_find",
        description = "Workflow step 2: find bounded candidate cards for a plain-language task. Run before opening source files."
    )]
    async fn synrepo_find(
        &self,
        Parameters(params): Parameters<search::WhereToEditParams>,
    ) -> String {
        self.record_workflow("find");
        search::handle_where_to_edit(
            &self.resolve_state(None),
            params.task,
            params.limit,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_explain",
        description = "Workflow step 3: bounded card lookup for a file or symbol. Prefer this over a full-file read; full-file reads are an explicit escalation."
    )]
    async fn synrepo_explain(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        self.record_workflow("explain");
        cards::handle_card(
            &self.resolve_state(params.repo_root.clone()),
            params.target,
            params.budget,
            params.budget_tokens,
            params.include_notes,
        )
    }

    #[tool(
        name = "synrepo_impact",
        description = "Workflow step 4: risk assessment before editing."
    )]
    async fn synrepo_impact(
        &self,
        Parameters(params): Parameters<cards::ChangeRiskParams>,
    ) -> String {
        self.record_workflow("impact");
        cards::handle_change_risk(
            &self.resolve_state(None),
            params.target,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_risks",
        description = "Workflow step 4 (shorthand): risk assessment before editing. Same output as synrepo_impact."
    )]
    async fn synrepo_risks(
        &self,
        Parameters(params): Parameters<cards::ChangeRiskParams>,
    ) -> String {
        self.record_workflow("risks");
        cards::handle_change_risk(
            &self.resolve_state(None),
            params.target,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_tests",
        description = "Workflow step 5: test discovery before claiming done."
    )]
    async fn synrepo_tests(
        &self,
        Parameters(params): Parameters<cards::TestSurfaceParams>,
    ) -> String {
        self.record_workflow("tests");
        cards::handle_test_surface(
            &self.resolve_state(None),
            params.scope,
            params.budget,
            params.budget_tokens,
        )
    }

    #[tool(
        name = "synrepo_changed",
        description = "Workflow step 6: changed-context review after edits. Use to confirm validation commands and changed files before handoff."
    )]
    async fn synrepo_changed(&self) -> String {
        self.record_workflow("changed");
        search::handle_changed(&self.resolve_state(None))
    }

    #[tool(
        name = "synrepo_refresh_commentary",
        description = "Explicitly generate or refresh LLM-authored commentary for a symbol. Use when synrepo_card reports commentary_state: 'missing' or 'stale' and fresh prose is required."
    )]
    async fn synrepo_refresh_commentary(
        &self,
        Parameters(params): Parameters<cards::RefreshCommentaryParams>,
    ) -> String {
        cards::handle_refresh_commentary(&self.resolve_state(None), params.target)
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
            &self.resolve_state(None).repo_root,
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
        audit::handle_recent_activity(
            &self.resolve_state(None),
            params.kinds,
            params.limit,
            params.since,
        )
    }

    #[tool(
        name = "synrepo_prepare_edit_context",
        description = "Edit-enabled workflow step: prepare session-scoped line anchors and compact source context for a file, symbol, or range. This tool does not write files; source mutation can occur only through synrepo_apply_anchor_edits."
    )]
    async fn synrepo_prepare_edit_context(
        &self,
        Parameters(params): Parameters<edits::PrepareEditContextParams>,
    ) -> String {
        edits::handle_prepare_edit_context(&self.resolve_state(params.repo_root.clone()), params)
    }

    #[tool(
        name = "synrepo_apply_anchor_edits",
        description = "Edit-enabled workflow step: validate prepared anchors, content hashes, and boundary text before applying source edits. This tool can mutate source files only when the server was started with synrepo mcp --allow-edits."
    )]
    async fn synrepo_apply_anchor_edits(
        &self,
        Parameters(params): Parameters<edits::ApplyAnchorEditsParams>,
    ) -> String {
        edits::handle_apply_anchor_edits(&self.resolve_state(params.repo_root.clone()), params)
    }

    #[tool(
        name = "synrepo_next_actions",
        description = "Return prioritized actionable items from repair-log, cross-link candidates, and git hotspots."
    )]
    async fn synrepo_next_actions(
        &self,
        Parameters(params): Parameters<audit::NextActionsParams>,
    ) -> String {
        let request = HandoffsRequest {
            limit: params.limit.unwrap_or(20),
            since_days: params.since_days.unwrap_or(30),
        };
        match collect_handoffs(
            &self.resolve_state(None).repo_root,
            &self.resolve_state(None).config,
            &request,
        ) {
            Ok(items) => handoffs_to_json(&items),
            Err(e) => serde_json::json!({
                "error": e.to_string()
            })
            .to_string(),
        }
    }
}
