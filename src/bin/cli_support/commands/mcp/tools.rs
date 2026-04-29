use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use synrepo::surface::handoffs::{collect_handoffs, to_json as handoffs_to_json, HandoffsRequest};
use synrepo::surface::mcp::{audit, cards, context_pack, docs, edits, notes, primitives, search};

use super::SynrepoServer;

#[rustfmt::skip]
#[tool_router]
impl SynrepoServer {
    #[tool(name = "synrepo_card", description = "Return a structured card describing a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_card(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_card(&state, params.target, params.budget, params.budget_tokens, params.include_notes))
    }

    #[tool(name = "synrepo_search", description = "Search the repository using lexical queries.")]
    async fn synrepo_search(&self, Parameters(params): Parameters<search::SearchParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| search::handle_search(&state, params.query, params.limit))
    }

    #[tool(name = "synrepo_docs_search", description = "Search advisory explained commentary docs materialized under .synrepo/. Results are overlay-backed, freshness-labeled, and never canonical graph facts.")]
    async fn synrepo_docs_search(&self, Parameters(params): Parameters<docs::DocsSearchParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| docs::handle_docs_search(&state, params.query, params.limit))
    }

    #[tool(name = "synrepo_context_pack", description = "Batch read-only context artifacts (file outlines, cards, neighborhoods, tests, call paths, and search) into one token-accounted response. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_context_pack(&self, Parameters(params): Parameters<context_pack::ContextPackParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| context_pack::handle_context_pack(&state, params))
    }

    #[tool(name = "synrepo_overview", description = "Return a high-level overview of the repository graph state.")]
    async fn synrepo_overview(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| search::handle_overview(&state))
    }

    #[tool(name = "synrepo_node", description = "Look up a graph node by display ID. Returns full stored metadata as JSON.")]
    async fn synrepo_node(&self, Parameters(params): Parameters<primitives::NodeParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| primitives::handle_node(&state, params.id))
    }

    #[tool(name = "synrepo_edges", description = "Traverse edges from a node. Optional direction (outbound/inbound) and edge type filters.")]
    async fn synrepo_edges(&self, Parameters(params): Parameters<primitives::EdgesParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| primitives::handle_edges(&state, params.id, params.direction, params.edge_types))
    }

    #[tool(name = "synrepo_query", description = "Structured graph query: 'outbound <node_id> [edge_kind]' or 'inbound <node_id> [edge_kind]'.")]
    async fn synrepo_query(&self, Parameters(params): Parameters<primitives::QueryParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| primitives::handle_query(&state, params.query))
    }

    #[tool(name = "synrepo_overlay", description = "Inspect overlay data for a node: commentary and proposed cross-links. Returns {overlay: null} when none exists.")]
    async fn synrepo_overlay(&self, Parameters(params): Parameters<primitives::OverlayParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| primitives::handle_overlay(&state, params.id))
    }

    #[tool(name = "synrepo_provenance", description = "Audit provenance for a node and its incident edges: source, created_by, source_ref for each.")]
    async fn synrepo_provenance(&self, Parameters(params): Parameters<primitives::ProvenanceParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| primitives::handle_provenance(&state, params.id))
    }

    #[tool(name = "synrepo_where_to_edit", description = "Suggest where to make edits for a plain-language task description. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_where_to_edit(&self, Parameters(params): Parameters<search::WhereToEditParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| search::handle_where_to_edit(&state, params.task, params.limit, params.budget_tokens))
    }

    #[tool(name = "synrepo_change_impact", description = "Assess the change impact of modifying a file or symbol. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_change_impact(&self, Parameters(params): Parameters<search::ChangeImpactParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| search::handle_change_impact(&state, params.target))
    }

    #[tool(name = "synrepo_entrypoints", description = "Return detected execution entry points (binaries, CLI commands, HTTP handlers, library roots) for an optional path-prefix scope. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_entrypoints(&self, Parameters(params): Parameters<cards::EntrypointsParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_entrypoints(&state, params.scope, params.budget, params.budget_tokens))
    }

    #[tool(name = "synrepo_note_add", description = "Add an advisory overlay agent note. Notes are labeled source_store=overlay and advisory=true; they never define graph truth.")]
    async fn synrepo_note_add(&self, Parameters(params): Parameters<notes::NoteAddParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| notes::handle_note_add(&state, params))
    }

    #[tool(name = "synrepo_note_link", description = "Link two advisory overlay notes while preserving audit history.")]
    async fn synrepo_note_link(&self, Parameters(params): Parameters<notes::NoteLinkParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| notes::handle_note_link(&state, params))
    }

    #[tool(name = "synrepo_note_supersede", description = "Supersede an advisory overlay note with a replacement claim.")]
    async fn synrepo_note_supersede(&self, Parameters(params): Parameters<notes::NoteSupersedeParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| notes::handle_note_supersede(&state, params))
    }

    #[tool(name = "synrepo_note_forget", description = "Hide an advisory overlay note from normal retrieval while retaining audit history.")]
    async fn synrepo_note_forget(&self, Parameters(params): Parameters<notes::NoteForgetParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| notes::handle_note_forget(&state, params))
    }

    #[tool(name = "synrepo_note_verify", description = "Verify an advisory overlay note and return it to active state when anchors match.")]
    async fn synrepo_note_verify(&self, Parameters(params): Parameters<notes::NoteVerifyParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| notes::handle_note_verify(&state, params))
    }

    #[tool(name = "synrepo_notes", description = "List bounded advisory overlay notes. Hidden lifecycle states require include_hidden=true.")]
    async fn synrepo_notes(&self, Parameters(params): Parameters<notes::NotesParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| notes::handle_notes(&state, params))
    }

    #[tool(name = "synrepo_module_card", description = "Return a ModuleCard summarizing a directory: files, nested modules, public symbols, and token budget. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_module_card(&self, Parameters(params): Parameters<cards::ModuleCardParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_module_card(&state, params.path, params.budget, params.budget_tokens))
    }

    #[tool(name = "synrepo_public_api", description = "Return a PublicAPICard for a directory: public symbols with kinds and signatures, public entry points, and (at deep budget) recently changed public API surface. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_public_api(&self, Parameters(params): Parameters<cards::PublicAPICardParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_public_api(&state, params.path, params.budget, params.budget_tokens))
    }

    #[tool(name = "synrepo_minimum_context", description = "Bounded neighborhood step for a focal symbol or file: focal card, outbound structural neighbors, governing decisions, and co-change partners. Use before deep cards or full-file reads when a target is known but surrounding risk is unclear. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_minimum_context(&self, Parameters(params): Parameters<cards::MinimumContextParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "minimum_context");
            cards::handle_minimum_context(&state, params.target, params.budget, params.budget_tokens)
        })
    }

    #[tool(name = "synrepo_call_path", description = "Return a CallPathCard tracing execution paths from entry points to a target symbol using backward BFS over Calls edges. Use to understand how to reach a function from binary/CLI/HTTP entry points. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_call_path(&self, Parameters(params): Parameters<cards::CallPathParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_call_path(&state, params.target, params.budget, params.budget_tokens))
    }

    #[tool(name = "synrepo_test_surface", description = "Return a TestSurfaceCard discovering test functions related to a file or directory scope (beta fidelity). Uses path-convention heuristics to associate test files with source files. Default budget is tiny; escalate to normal for local understanding and deep only before edits.")]
    async fn synrepo_test_surface(&self, Parameters(params): Parameters<cards::TestSurfaceParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_test_surface(&state, params.scope, params.budget, params.budget_tokens))
    }

    #[tool(name = "synrepo_change_risk", description = "Return a change risk assessment for a symbol or file (beta fidelity), aggregating drift score, co-change partners, and git hotspot data.")]
    async fn synrepo_change_risk(&self, Parameters(params): Parameters<cards::ChangeRiskParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_change_risk(&state, params.target, params.budget, params.budget_tokens))
    }

    #[tool(name = "synrepo_orient", description = "Workflow step 1: orient before reading the repo cold. Run before any cold file reads.")]
    async fn synrepo_orient(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "orient");
            search::handle_overview(&state)
        })
    }

    #[tool(name = "synrepo_find", description = "Workflow step 2: find bounded candidate cards for a plain-language task. Run before opening source files.")]
    async fn synrepo_find(&self, Parameters(params): Parameters<search::WhereToEditParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "find");
            search::handle_where_to_edit(&state, params.task, params.limit, params.budget_tokens)
        })
    }

    #[tool(name = "synrepo_explain", description = "Workflow step 3: bounded card lookup for a file or symbol. Prefer this over a full-file read; full-file reads are an explicit escalation.")]
    async fn synrepo_explain(&self, Parameters(params): Parameters<cards::CardParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "explain");
            cards::handle_card(&state, params.target, params.budget, params.budget_tokens, params.include_notes)
        })
    }

    #[tool(name = "synrepo_impact", description = "Workflow step 4: risk assessment before editing.")]
    async fn synrepo_impact(&self, Parameters(params): Parameters<cards::ChangeRiskParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "impact");
            cards::handle_change_risk(&state, params.target, params.budget, params.budget_tokens)
        })
    }

    #[tool(name = "synrepo_risks", description = "Workflow step 4 (shorthand): risk assessment before editing. Same output as synrepo_impact.")]
    async fn synrepo_risks(&self, Parameters(params): Parameters<cards::ChangeRiskParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "risks");
            cards::handle_change_risk(&state, params.target, params.budget, params.budget_tokens)
        })
    }

    #[tool(name = "synrepo_tests", description = "Workflow step 5: test discovery before claiming done.")]
    async fn synrepo_tests(&self, Parameters(params): Parameters<cards::TestSurfaceParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "tests");
            cards::handle_test_surface(&state, params.scope, params.budget, params.budget_tokens)
        })
    }

    #[tool(name = "synrepo_changed", description = "Workflow step 6: changed-context review after edits. Use to confirm validation commands and changed files before handoff.")]
    async fn synrepo_changed(&self, Parameters(params): Parameters<search::RepoRootParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| {
            self.record_workflow_for(&state, "changed");
            search::handle_changed(&state)
        })
    }

    #[tool(name = "synrepo_refresh_commentary", description = "Explicitly generate or refresh LLM-authored commentary for a symbol. Use when synrepo_card reports commentary_state: 'missing' or 'stale' and fresh prose is required.")]
    async fn synrepo_refresh_commentary(&self, Parameters(params): Parameters<cards::RefreshCommentaryParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| cards::handle_refresh_commentary(&state, params.target))
    }

    #[tool(name = "synrepo_findings", description = "List operator-facing cross-link findings with provenance, tier, score, freshness, and endpoint IDs.")]
    async fn synrepo_findings(&self, Parameters(params): Parameters<audit::FindingsParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| audit::handle_findings(&state.repo_root, params.node_id, params.kind, params.freshness, params.limit))
    }

    #[tool(name = "synrepo_recent_activity", description = "Return bounded operational activity (beta fidelity): reconcile outcomes, repair events, cross-link audit entries, commentary refreshes, and git hotspots. NOT a session-memory or agent-interaction log.")]
    async fn synrepo_recent_activity(&self, Parameters(params): Parameters<audit::RecentActivityParams>) -> String {
        self.with_state(params.repo_root.clone(), |state| audit::handle_recent_activity(&state, params.kinds, params.limit, params.since))
    }

    #[tool(name = "synrepo_prepare_edit_context", description = "Edit-enabled workflow step: prepare session-scoped line anchors and compact source context for a file, symbol, or range. This tool does not write files; source mutation can occur only through synrepo_apply_anchor_edits.")]
    async fn synrepo_prepare_edit_context(&self, Parameters(params): Parameters<edits::PrepareEditContextParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| edits::handle_prepare_edit_context(&state, params))
    }

    #[tool(name = "synrepo_apply_anchor_edits", description = "Edit-enabled workflow step: validate prepared anchors, content hashes, and boundary text before applying source edits. This tool can mutate source files only when the server was started with synrepo mcp --allow-edits.")]
    async fn synrepo_apply_anchor_edits(&self, Parameters(params): Parameters<edits::ApplyAnchorEditsParams>) -> String {
        let repo_root = params.repo_root.clone();
        self.with_state(repo_root, |state| edits::handle_apply_anchor_edits(&state, params))
    }

    #[tool(name = "synrepo_next_actions", description = "Return prioritized actionable items from repair-log, cross-link candidates, and git hotspots.")]
    async fn synrepo_next_actions(&self, Parameters(params): Parameters<audit::NextActionsParams>) -> String {
        let request = HandoffsRequest { limit: params.limit.unwrap_or(20), since_days: params.since_days.unwrap_or(30) };
        self.with_state(params.repo_root.clone(), |state| match collect_handoffs(&state.repo_root, &state.config, &request) {
            Ok(items) => handoffs_to_json(&items),
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        })
    }
}

impl SynrepoServer {
    pub(super) fn build_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        Self::tool_router()
    }
}
