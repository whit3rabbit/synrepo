use rmcp::{
    handler::server::wrapper::Parameters,
    model::{Meta, ProgressNotificationParam},
    tool, tool_router, Peer,
};
use synrepo::surface::handoffs::{collect_handoffs, to_json as handoffs_to_json, HandoffsRequest};
use synrepo::surface::mcp::{
    audit, card_batch, cards, commentary, context_pack, docs, edits, graph, notes, primitives,
    refactor_suggestions, search, task_route,
};

use super::SynrepoServer;

#[rustfmt::skip]
#[tool_router]
impl SynrepoServer {
    #[tool(name = "synrepo_card", description = "Return a structured card describing a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_card(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        let repo_root = params.repo_root.clone();
        if let Err(error) = self.resolve_state(repo_root.clone()) {
            return card_batch::handle_degraded_card(repo_root, params, error);
        }
        self.with_tool_state_blocking("synrepo_card", params.repo_root.clone(), move |state| card_batch::handle_card_params(&state, params)).await
    }

    #[tool(name = "synrepo_search", description = "Search the repository using lexical queries. Best for exact symbols, string literals, CLI flags, MCP tool names, schema keys, file paths, and code-review validation. Prefer output_mode=\"compact\" for orientation, then use suggested_card_targets with synrepo_card.")]
    async fn synrepo_search(&self, Parameters(params): Parameters<search::SearchParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_search", repo_root, move |state| search::handle_search(&state, params)).await
    }

    #[tool(name = "synrepo_task_route", description = "Classify a task into the cheapest safe synrepo route. Read-only and advisory: returns intent, confidence, recommended tools, budget tier, LLM requirement, edit candidate, and hook signals.")]
    async fn synrepo_task_route(&self, Parameters(params): Parameters<task_route::TaskRouteParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_task_route", repo_root, move |state| task_route::handle_task_route(&state, params)).await
    }

    #[tool(name = "synrepo_docs_search", description = "Search advisory explained commentary docs materialized under .synrepo/. Results are overlay-backed, freshness-labeled, and never canonical graph facts.")]
    async fn synrepo_docs_search(&self, Parameters(params): Parameters<docs::DocsSearchParams>) -> String {
        self.with_tool_state_blocking("synrepo_docs_search", params.repo_root.clone(), move |state| docs::handle_docs_search(&state, params.query, params.limit)).await
    }

    #[tool(name = "synrepo_context_pack", description = "Batch read-only context artifacts into one token-accounted response. Pass targets as structured objects: {kind,target,budget?}. Kinds: file, symbol, directory, minimum_context, test_surface, call_path, search. Use output_mode=\"compact\" to compact search artifacts; card artifacts keep context_accounting. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_context_pack(&self, Parameters(params): Parameters<context_pack::ContextPackParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_context_pack", repo_root, move |state| context_pack::handle_context_pack(&state, params)).await
    }

    #[tool(name = "synrepo_overview", description = "Return a high-level overview of the repository graph state.")]
    async fn synrepo_overview(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        if let Some(repo_root) = params.repo_root.clone() {
            if let Err(error) = self.resolve_state(Some(repo_root.clone())) {
                return search::handle_degraded_overview(repo_root, error);
            }
        }
        self.with_tool_state_blocking("synrepo_overview", params.repo_root.clone(), move |state| search::handle_overview(&state)).await
    }

    #[tool(name = "synrepo_use_project", description = "Set the default repository root for this MCP server session. Useful for global/defaultless MCP configs.")]
    async fn synrepo_use_project(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        match params.repo_root {
            Some(repo_root) => self.use_project(repo_root),
            None => super::state::render_state_error(
                synrepo::surface::mcp::error::McpError::invalid_parameter("repo_root is required").into(),
            ),
        }
    }

    #[tool(name = "synrepo_metrics", description = "Return this MCP server session's tool metrics plus persisted per-repo context metrics when a repo is available.")]
    async fn synrepo_metrics(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        let state = self.resolve_state(params.repo_root.clone()).ok();
        self.metrics_json(state.as_deref())
    }

    #[tool(name = "synrepo_node", description = "Look up a graph node by display ID. Returns full stored metadata as JSON.")]
    async fn synrepo_node(&self, Parameters(params): Parameters<primitives::NodeParams>) -> String {
        self.with_tool_state_blocking("synrepo_node", params.repo_root.clone(), move |state| primitives::handle_node(&state, params.id)).await
    }

    #[tool(name = "synrepo_edges", description = "Traverse edges from a node. Optional direction (outbound/inbound) and edge type filters.")]
    async fn synrepo_edges(&self, Parameters(params): Parameters<primitives::EdgesParams>) -> String {
        self.with_tool_state_blocking("synrepo_edges", params.repo_root.clone(), move |state| primitives::handle_edges(&state, params.id, params.direction, params.edge_types)).await
    }

    #[tool(name = "synrepo_query", description = "Structured graph query: 'outbound <target> [edge_kind]' or 'inbound <target> [edge_kind]'. Target accepts node IDs, file paths, and symbol names.")]
    async fn synrepo_query(&self, Parameters(params): Parameters<primitives::QueryParams>) -> String {
        self.with_tool_state_blocking("synrepo_query", params.repo_root.clone(), move |state| primitives::handle_query(&state, params.query)).await
    }

    #[tool(name = "synrepo_graph_neighborhood", description = "Return a bounded graph-backed neighborhood model for a target, or a top-degree overview when target is omitted.")]
    async fn synrepo_graph_neighborhood(&self, Parameters(params): Parameters<graph::GraphNeighborhoodParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_graph_neighborhood", repo_root, move |state| graph::handle_graph_neighborhood(&state, params)).await
    }

    #[tool(name = "synrepo_overlay", description = "Inspect overlay data for a node: commentary and proposed cross-links. Returns {overlay: null} when none exists.")]
    async fn synrepo_overlay(&self, Parameters(params): Parameters<primitives::OverlayParams>) -> String {
        self.with_tool_state_blocking("synrepo_overlay", params.repo_root.clone(), move |state| primitives::handle_overlay(&state, params.id)).await
    }

    #[tool(name = "synrepo_provenance", description = "Audit provenance for a node and its incident edges: source, created_by, source_ref for each.")]
    async fn synrepo_provenance(&self, Parameters(params): Parameters<primitives::ProvenanceParams>) -> String {
        self.with_tool_state_blocking("synrepo_provenance", params.repo_root.clone(), move |state| primitives::handle_provenance(&state, params.id)).await
    }

    #[tool(name = "synrepo_where_to_edit", description = "Suggest where to make edits for a plain-language task description. Best for plain-language task routing, not exact code symbols, string literals, flags, schema fields, tool names, or file paths. If the user mentions exact identifiers, call synrepo_search first. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_where_to_edit(&self, Parameters(params): Parameters<search::WhereToEditParams>) -> String {
        self.with_tool_state_blocking("synrepo_where_to_edit", params.repo_root.clone(), move |state| search::handle_where_to_edit(&state, params.task, params.limit, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_change_impact", description = "Assess the change impact of modifying a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_change_impact(&self, Parameters(params): Parameters<search::ChangeImpactParams>) -> String {
        self.with_tool_state_blocking("synrepo_change_impact", params.repo_root.clone(), move |state| search::handle_change_impact_with_direction(&state, params.target, params.direction)).await
    }

    #[tool(name = "synrepo_entrypoints", description = "Return detected execution entry points (binaries, CLI commands, HTTP handlers, library roots) for an optional path-prefix scope. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_entrypoints(&self, Parameters(params): Parameters<cards::EntrypointsParams>) -> String {
        self.with_tool_state_blocking("synrepo_entrypoints", params.repo_root.clone(), move |state| cards::handle_entrypoints(&state, params.scope, params.budget, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_refactor_suggestions", description = "Suggest large non-test source files that may benefit from modular refactors. Returns deterministic file facts and lightweight modularity hints for LLM analysis.")]
    async fn synrepo_refactor_suggestions(&self, Parameters(params): Parameters<refactor_suggestions::RefactorSuggestionsParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_refactor_suggestions", repo_root, move |state| refactor_suggestions::handle_refactor_suggestions(&state, params)).await
    }

    #[tool(name = "synrepo_note_add", description = "Add an advisory overlay agent note. Notes are labeled source_store=overlay and advisory=true; they never define graph truth.")]
    async fn synrepo_note_add(&self, Parameters(params): Parameters<notes::NoteAddParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_note_add", repo_root, move |state| notes::handle_note_add(&state, params)).await
    }

    #[tool(name = "synrepo_note_link", description = "Link two advisory overlay notes while preserving audit history.")]
    async fn synrepo_note_link(&self, Parameters(params): Parameters<notes::NoteLinkParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_note_link", repo_root, move |state| notes::handle_note_link(&state, params)).await
    }

    #[tool(name = "synrepo_note_supersede", description = "Supersede an advisory overlay note with a replacement claim.")]
    async fn synrepo_note_supersede(&self, Parameters(params): Parameters<notes::NoteSupersedeParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_note_supersede", repo_root, move |state| notes::handle_note_supersede(&state, params)).await
    }

    #[tool(name = "synrepo_note_forget", description = "Hide an advisory overlay note from normal retrieval while retaining audit history.")]
    async fn synrepo_note_forget(&self, Parameters(params): Parameters<notes::NoteForgetParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_note_forget", repo_root, move |state| notes::handle_note_forget(&state, params)).await
    }

    #[tool(name = "synrepo_note_verify", description = "Verify an advisory overlay note and return it to active state when anchors match.")]
    async fn synrepo_note_verify(&self, Parameters(params): Parameters<notes::NoteVerifyParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_note_verify", repo_root, move |state| notes::handle_note_verify(&state, params)).await
    }

    #[tool(name = "synrepo_notes", description = "List bounded advisory overlay notes. Hidden lifecycle states require include_hidden=true.")]
    async fn synrepo_notes(&self, Parameters(params): Parameters<notes::NotesParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_notes", repo_root, move |state| notes::handle_notes(&state, params)).await
    }

    #[tool(name = "synrepo_module_card", description = "Return a ModuleCard summarizing a directory: files, nested modules, public symbols, and token budget. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_module_card(&self, Parameters(params): Parameters<cards::ModuleCardParams>) -> String {
        self.with_tool_state_blocking("synrepo_module_card", params.repo_root.clone(), move |state| cards::handle_module_card(&state, params.path, params.budget, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_public_api", description = "Return a PublicAPICard for a directory: public symbols with kinds and signatures, public entry points, and (at deep budget) recently changed public API surface. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_public_api(&self, Parameters(params): Parameters<cards::PublicAPICardParams>) -> String {
        self.with_tool_state_blocking("synrepo_public_api", params.repo_root.clone(), move |state| cards::handle_public_api(&state, params.path, params.budget, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_minimum_context", description = "Bounded neighborhood step for a focal symbol or file: focal card, outbound structural neighbors, governing decisions, and co-change partners. Use before deep cards or full-file reads when a target is known but surrounding risk is unclear. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_minimum_context(&self, Parameters(params): Parameters<cards::MinimumContextParams>) -> String {
        self.with_tool_state_blocking("synrepo_minimum_context", params.repo_root.clone(), move |state| {
            record_workflow(&state, "minimum_context");
            cards::handle_minimum_context(&state, params.target, params.budget, params.budget_tokens)
        }).await
    }

    #[tool(name = "synrepo_call_path", description = "Return a CallPathCard tracing execution paths from entry points to a target symbol using backward BFS over Calls edges. Use to understand how to reach a function from binary/CLI/HTTP entry points. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_call_path(&self, Parameters(params): Parameters<cards::CallPathParams>) -> String {
        self.with_tool_state_blocking("synrepo_call_path", params.repo_root.clone(), move |state| cards::handle_call_path(&state, params.target, params.budget, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_test_surface", description = "Return a TestSurfaceCard discovering test functions related to a file or directory scope (beta fidelity). Uses path-convention heuristics to associate test files with source files. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_test_surface(&self, Parameters(params): Parameters<cards::TestSurfaceParams>) -> String {
        self.with_tool_state_blocking("synrepo_test_surface", params.repo_root.clone(), move |state| cards::handle_test_surface(&state, params.scope, params.budget, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_change_risk", description = "Return a change risk assessment for a symbol or file (beta fidelity), aggregating drift score, co-change partners, and git hotspot data.")]
    async fn synrepo_change_risk(&self, Parameters(params): Parameters<cards::ChangeRiskParams>) -> String {
        self.with_tool_state_blocking("synrepo_change_risk", params.repo_root.clone(), move |state| cards::handle_change_risk(&state, params.target, params.budget, params.budget_tokens)).await
    }

    #[tool(name = "synrepo_orient", description = "Workflow step 1: orient before reading the repo cold. Run before any cold file reads.")]
    async fn synrepo_orient(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        self.with_tool_state_blocking("synrepo_orient", params.repo_root.clone(), move |state| {
            record_workflow(&state, "orient");
            search::handle_overview(&state)
        }).await
    }

    #[tool(name = "synrepo_find", description = "Workflow step 2: find bounded candidate cards for a plain-language task. Best for plain-language task routing, not exact code symbols, string literals, flags, schema fields, tool names, or file paths. If the user mentions exact identifiers, call synrepo_search first.")]
    async fn synrepo_find(&self, Parameters(params): Parameters<search::WhereToEditParams>) -> String {
        self.with_tool_state_blocking("synrepo_find", params.repo_root.clone(), move |state| {
            record_workflow(&state, "find");
            search::handle_where_to_edit(&state, params.task, params.limit, params.budget_tokens)
        }).await
    }

    #[tool(name = "synrepo_explain", description = "Workflow step 3: bounded card lookup for a file or symbol. Prefer this over a full-file read; full-file reads are an explicit escalation.")]
    async fn synrepo_explain(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        self.with_tool_state_blocking("synrepo_explain", params.repo_root.clone(), move |state| {
            record_workflow(&state, "explain");
            card_batch::handle_card_params(&state, params)
        }).await
    }

    #[tool(name = "synrepo_impact", description = "Workflow step 4: risk assessment before editing.")]
    async fn synrepo_impact(&self, Parameters(params): Parameters<cards::ChangeRiskParams>) -> String {
        self.with_tool_state_blocking("synrepo_impact", params.repo_root.clone(), move |state| {
            record_workflow(&state, "impact");
            cards::handle_change_risk(&state, params.target, params.budget, params.budget_tokens)
        }).await
    }

    #[tool(name = "synrepo_risks", description = "Workflow step 4 (shorthand): risk assessment before editing. Same output as synrepo_impact.")]
    async fn synrepo_risks(&self, Parameters(params): Parameters<cards::ChangeRiskParams>) -> String {
        self.with_tool_state_blocking("synrepo_risks", params.repo_root.clone(), move |state| {
            record_workflow(&state, "risks");
            cards::handle_change_risk(&state, params.target, params.budget, params.budget_tokens)
        }).await
    }

    #[tool(name = "synrepo_tests", description = "Workflow step 5: test discovery before claiming done.")]
    async fn synrepo_tests(&self, Parameters(params): Parameters<cards::TestSurfaceParams>) -> String {
        self.with_tool_state_blocking("synrepo_tests", params.repo_root.clone(), move |state| {
            record_workflow(&state, "tests");
            cards::handle_test_surface(&state, params.scope, params.budget, params.budget_tokens)
        }).await
    }

    #[tool(name = "synrepo_changed", description = "Workflow step 6: changed-context review after edits. Use to confirm validation commands and changed files before handoff.")]
    async fn synrepo_changed(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        self.with_tool_state_blocking("synrepo_changed", params.repo_root.clone(), move |state| {
            record_workflow(&state, "changed");
            search::handle_changed(&state)
        }).await
    }

    #[tool(name = "synrepo_refresh_commentary", description = "Explicitly generate or refresh LLM-authored commentary for a target, file, directory, or all stale entries. Use when synrepo_card reports commentary_state: 'missing' or 'stale' and fresh prose is required.")]
    async fn synrepo_refresh_commentary(&self, Parameters(params): Parameters<commentary::RefreshCommentaryParams>, meta: Meta, client: Peer<rmcp::RoleServer>) -> String {
        let progress_token = meta.get_progress_token();
        if let Some(token) = progress_token.clone() {
            let _ = client.notify_progress(ProgressNotificationParam {
                progress_token: token,
                progress: 0.0,
                total: Some(1.0),
                message: Some("refreshing commentary".into()),
            }).await;
        }
        let output = self.with_tool_state_blocking("synrepo_refresh_commentary", params.repo_root.clone(), move |state| {
            commentary::handle_refresh_commentary_params(&state, params, None)
        }).await;
        if let Some(token) = progress_token {
            let _ = client.notify_progress(ProgressNotificationParam {
                progress_token: token,
                progress: 1.0,
                total: Some(1.0),
                message: Some("commentary refresh complete".into()),
            }).await;
        }
        output
    }

    #[tool(name = "synrepo_findings", description = "List operator-facing cross-link findings with provenance, tier, score, freshness, and endpoint IDs.")]
    async fn synrepo_findings(&self, Parameters(params): Parameters<audit::FindingsParams>) -> String {
        self.with_tool_state_blocking("synrepo_findings", params.repo_root.clone(), move |state| audit::handle_findings(&state.repo_root, params.node_id, params.kind, params.freshness, params.limit)).await
    }

    #[tool(name = "synrepo_recent_activity", description = "Return bounded operational activity (beta fidelity): reconcile outcomes, repair events, cross-link audit entries, commentary refreshes, and git hotspots. NOT a session-memory or agent-interaction log.")]
    async fn synrepo_recent_activity(&self, Parameters(params): Parameters<audit::RecentActivityParams>) -> String {
        self.with_tool_state_blocking("synrepo_recent_activity", params.repo_root.clone(), move |state| audit::handle_recent_activity(&state, params.kinds, params.limit, params.since)).await
    }

    #[tool(name = "synrepo_prepare_edit_context", description = "Edit-enabled workflow step: prepare session-scoped line anchors and compact source context for a file, symbol, or range. This tool does not write files; source mutation can occur only through synrepo_apply_anchor_edits.")]
    async fn synrepo_prepare_edit_context(&self, Parameters(params): Parameters<edits::PrepareEditContextParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_prepare_edit_context", repo_root, move |state| edits::handle_prepare_edit_context(&state, params)).await
    }

    #[tool(name = "synrepo_apply_anchor_edits", description = "Edit-enabled workflow step: validate prepared anchors, content hashes, and boundary text before applying source edits. This tool can mutate source files only when the server was started with synrepo mcp --allow-source-edits.")]
    async fn synrepo_apply_anchor_edits(&self, Parameters(params): Parameters<edits::ApplyAnchorEditsParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_tool_state_blocking("synrepo_apply_anchor_edits", repo_root, move |state| edits::handle_apply_anchor_edits(&state, params)).await
    }

    #[tool(name = "synrepo_next_actions", description = "Return prioritized actionable items from repair-log, cross-link candidates, and git hotspots.")]
    async fn synrepo_next_actions(&self, Parameters(params): Parameters<audit::NextActionsParams>) -> String {
        let request = HandoffsRequest { limit: params.limit.unwrap_or(20), since_days: params.since_days.unwrap_or(30) };
        self.with_tool_state_blocking("synrepo_next_actions", params.repo_root.clone(), move |state| match collect_handoffs(&state.repo_root, &state.config, &request) {
            Ok(items) => handoffs_to_json(&items),
            Err(e) => synrepo::surface::mcp::error::error_json(e.into()),
        }).await
    }
}

impl SynrepoServer {
    pub(super) fn build_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::tool_router()
    }
}

fn record_workflow(state: &synrepo::surface::mcp::SynrepoState, tool: &str) {
    let synrepo_dir = synrepo::config::Config::synrepo_dir(&state.repo_root);
    synrepo::pipeline::context_metrics::record_workflow_call_best_effort(&synrepo_dir, tool);
}
