## Purpose

### Requirement: Expose synrepo_node as a raw graph lookup tool
The MCP server SHALL provide a `synrepo_node` tool that accepts a node ID string and returns the full stored metadata for that node as JSON. The node ID SHALL be parsed using the display-format convention (`file_`, `sym_`, `concept_` prefix). Legacy `symbol_` IDs SHALL be accepted as input aliases for canonical `sym_` IDs. If the ID does not parse or no node exists, the tool SHALL return a structured error.

#### Scenario: Agent looks up a file node by display ID
- **WHEN** agent calls `synrepo_node` with `id = "file_0000000000000042"`
- **THEN** the tool returns JSON with the FileNode fields: id, path, language, content_hash, file_class, path_history, git_intelligence, provenance

#### Scenario: Agent looks up a symbol node by display ID
- **WHEN** agent calls `synrepo_node` with `id = "sym_0000000000000024"`
- **THEN** the tool returns JSON with the SymbolNode fields: id, file_id, qualified_name, kind, signature, doc_comment, body_hash, last_change, provenance

#### Scenario: Agent provides a legacy symbol node ID
- **WHEN** agent calls `synrepo_node` with `id = "symbol_0000000000000024"`
- **THEN** the tool resolves the legacy alias
- **AND** the response uses canonical `sym_` output

#### Scenario: Agent provides an invalid node ID
- **WHEN** agent calls `synrepo_node` with `id = "invalid_123"`
- **THEN** the tool returns an error message listing the valid ID prefixes (file_, sym_, concept_)

#### Scenario: Agent provides a valid ID for a non-existent node
- **WHEN** agent calls `synrepo_node` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found

### Requirement: Expose synrepo_edges as a raw edge traversal tool
The MCP server SHALL provide a `synrepo_edges` tool that accepts a node ID string, an optional direction (`outbound` or `inbound`, defaulting to `outbound`), and an optional list of edge type filters. It SHALL return all matching edges with their full metadata including provenance.

#### Scenario: Agent traverses outbound edges from a node
- **WHEN** agent calls `synrepo_edges` with `id = "file_0000000000000042"` and no direction
- **THEN** the tool returns all outbound edges from that node, each with edge_kind, target node ID, and provenance

#### Scenario: Agent traverses inbound edges filtered by type
- **WHEN** agent calls `synrepo_edges` with `id = "sym_0000000000000024"`, `direction = "inbound"`, and `edge_types = ["Calls"]`
- **THEN** the tool returns only inbound `Calls` edges targeting that symbol

#### Scenario: Agent traverses with multiple edge type filters
- **WHEN** agent calls `synrepo_edges` with `id = "file_0000000000000042"` and `edge_types = ["Defines", "Imports"]`
- **THEN** the tool returns only outbound edges of kind `Defines` or `Imports`

#### Scenario: Node has no matching edges
- **WHEN** agent calls `synrepo_edges` for a valid node that has no edges matching the filters
- **THEN** the tool returns an empty edges array

### Requirement: Expose synrepo_query as a structured graph query tool
The MCP server SHALL provide a `synrepo_query` tool that accepts a query string in the existing CLI graph query syntax (`outbound <target> [edge_kind]`, `inbound <target> [edge_kind]`) and returns the matching edges as JSON. This reuses the same query DSL already supported by the CLI `synrepo graph query` command. The `<target>` SHALL accept a node ID, file path, qualified symbol name, or short symbol name.

#### Scenario: Agent queries outbound edges with kind filter
- **WHEN** agent calls `synrepo_query` with `query = "outbound file_0000000000000042 Defines"`
- **THEN** the tool returns all `Defines` edges from that file node

#### Scenario: Agent queries inbound edges without kind filter
- **WHEN** agent calls `synrepo_query` with `query = "inbound sym_0000000000000024"`
- **THEN** the tool returns all inbound edges to that symbol

#### Scenario: Agent queries by symbol name
- **WHEN** agent calls `synrepo_query` with `query = "outbound handle_query"`
- **THEN** the tool resolves `handle_query` to its graph node
- **AND** the tool returns outbound edges from that node

#### Scenario: Agent provides a malformed query string
- **WHEN** agent calls `synrepo_query` with `query = "sideways file_123"`
- **THEN** the tool returns an error describing the expected syntax

### Requirement: Expose synrepo_graph_neighborhood as a bounded graph model tool
The MCP server SHALL provide `synrepo_graph_neighborhood` as a read-only graph-backed tool. The tool SHALL accept optional `target`, optional `direction` (`both`, `inbound`, `outbound`), optional `edge_types`, optional `depth`, and optional `limit`, and SHALL return the shared graph-neighborhood model.

#### Scenario: Agent requests a target-centered graph neighborhood
- **WHEN** an agent calls `synrepo_graph_neighborhood` with `target = "handle_query"`
- **THEN** the tool resolves the target using the same target-resolution behavior as cards and CLI graph query
- **AND** it returns bounded graph-backed nodes and edges with provenance and epistemic labels

#### Scenario: Agent requests graph overview
- **WHEN** an agent calls `synrepo_graph_neighborhood` without a target
- **THEN** the tool returns a deterministic top-degree overview bounded by the default limit

#### Scenario: Agent requests too much graph context
- **WHEN** an agent calls `synrepo_graph_neighborhood` with depth or limit above the supported maximum
- **THEN** synrepo clamps depth to 3 and limit to 500
- **AND** marks the response as truncated when records were omitted

### Requirement: Expose synrepo_overlay as an overlay inspection tool
The MCP server SHALL provide a `synrepo_overlay` tool that accepts a node ID string and returns all overlay data associated with that node: commentary entry (if present) and proposed links with their status and confidence. If no overlay data exists, the tool SHALL return `{"overlay": null}` to distinguish absence from an error.

#### Scenario: Agent inspects a node with commentary and proposed links
- **WHEN** agent calls `synrepo_overlay` with `id = "file_0000000000000042"` and overlay data exists
- **THEN** the tool returns the commentary entry (text, confidence, freshness) and all proposed links with status, confidence tier, and source/target spans

#### Scenario: Agent inspects a node with no overlay data
- **WHEN** agent calls `synrepo_overlay` with `id = "sym_0000000000000024"` and no overlay data exists
- **THEN** the tool returns `{"overlay": null}`

#### Scenario: Agent inspects a non-existent node
- **WHEN** agent calls `synrepo_overlay` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found in the graph

### Requirement: Expose synrepo_provenance as a provenance audit tool
The MCP server SHALL provide a `synrepo_provenance` tool that accepts a node ID string and returns the full provenance chain for that node and its incident edges. This includes the node's own provenance (source, created_by, source_ref) and for each incident edge, the edge's provenance and the peer node ID.

#### Scenario: Agent audits provenance for a node with edges
- **WHEN** agent calls `synrepo_provenance` with `id = "file_0000000000000042"`
- **THEN** the tool returns the node's provenance, plus a list of incident edges each with their provenance and the peer node ID

#### Scenario: Agent audits provenance for a node with no edges
- **WHEN** agent calls `synrepo_provenance` with `id = "concept_0000000000000099"` and the concept has no edges
- **THEN** the tool returns the node's provenance with an empty edges list

#### Scenario: Agent audits provenance for a non-existent node
- **WHEN** agent calls `synrepo_provenance` with `id = "file_9999999999999999"`
- **THEN** the tool returns an error stating the node was not found
Define the task-first MCP tools and response contracts that expose card-based synrepo behavior to coding agents.
## Requirements
### Requirement: Provide task-first MCP tools
synrepo SHALL define an MCP surface centered on task-first tools for orientation, card lookup, where-to-edit, change impact, entrypoints, call paths, test surface, minimum context, and findings.

#### Scenario: Route an edit from task language
- **WHEN** an agent asks where to edit for a task description
- **THEN** the MCP surface defines a task-first tool that returns bounded card-based results
- **AND** the tool contract does not require raw graph traversal knowledge from the caller

#### Scenario: Route miss recommends exact follow-up probes
- **WHEN** `synrepo_find` or `synrepo_where_to_edit` cannot produce graph-backed suggestions for broad task language
- **THEN** the response includes miss diagnostics (`query_attempts`, `fallback_used`, and `miss_reason`)
- **AND** the response MAY include `recommended_next_queries` plus `recommended_tool: "synrepo_search"` to guide exact lexical follow-up probes

### Requirement: Expose synrepo_readiness as a read-only preflight tool
synrepo SHALL expose `synrepo_readiness(repo_root?)` as a cheap read-only MCP tool for operational trust checks. The tool SHALL report top-level `graph`, `overlay`, `index`, `watch`, `reconcile`, and `edit_mode` fields. It SHALL include readiness detail rows derived from the shared readiness matrix. The tool SHALL NOT start watch, run reconcile, refresh commentary, mutate overlay state, mutate source files, or enable mutating tools.

#### Scenario: Agent checks readiness before relying on repo context
- **WHEN** an agent invokes `synrepo_readiness`
- **THEN** the response includes `graph`, `overlay`, `index`, `watch`, and `reconcile` status fields
- **AND** the response includes `edit_mode.overlay_writes` and `edit_mode.source_edits`
- **AND** the response includes capability detail rows with severity and next action

#### Scenario: Default server reports read-only edit modes
- **WHEN** the MCP server was started without overlay-write or source-edit flags
- **AND** an agent invokes `synrepo_readiness`
- **THEN** `edit_mode.overlay_writes` is false
- **AND** `edit_mode.source_edits` is false

#### Scenario: Readiness does not repair or refresh state
- **WHEN** `synrepo_readiness` observes a stale index, inactive watch, or missing overlay database
- **THEN** the response labels that state and recommends next actions where available
- **AND** the tool performs no write or background-start side effects

### Requirement: Expose synrepo_docs_search as an advisory docs-search tool
synrepo SHALL expose `synrepo_docs_search(query, limit?)` as an MCP tool that searches materialized explain commentary docs under `.synrepo/explain-docs/`. The tool SHALL return overlay-backed advisory results only, never canonical graph facts. Each result SHALL include `node_id`, `qualified_name`, `source_path`, `path`, `line`, `content`, `commentary_state`, `generated_at`, `model_identity`, and `source_store: "overlay"`.

#### Scenario: Agent searches explain commentary docs
- **WHEN** an agent invokes `synrepo_docs_search` with `query: "retry loop"` and `limit: 10`
- **THEN** the tool returns only commentary-doc matches from the explain-doc corpus
- **AND** each result is labeled as overlay-backed advisory content rather than graph-backed structure

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_docs_search` appears in the tool list
- **AND** the description states that the results are advisory explain docs, freshness-labeled, and not canonical graph facts

### Requirement: Require provenance and freshness in responses
synrepo SHALL define MCP response contracts that expose provenance, source-store labeling, and freshness behavior for all graph-backed and overlay-backed content. For overlay commentary specifically, the contract SHALL define the observable behavior for each commentary state: present-and-fresh, present-and-stale, absent, and budget-withheld. For overlay cross-link candidates specifically, the contract SHALL define the observable behavior for each candidate state: present-fresh-high-tier (included with source label and tier), present-fresh-review-tier (included at `deep` budget with explicit review-queue label), present-stale (included with staleness label regardless of tier), present-source-deleted (included with `source_deleted` label, indicating the candidate refers to a node that no longer exists), absent (no candidates exist for the queried node, label marked missing), and budget-withheld (candidates were not included due to budget; labeled as withheld, not as absent). Below-threshold candidates SHALL NOT appear in card-shaped MCP responses; they are retrievable only through the dedicated findings tool.

#### Scenario: Consume mixed-source context
- **WHEN** an MCP response contains both structural facts and optional overlay content
- **THEN** the caller can distinguish the source and freshness of each returned field
- **AND** the contract prevents silent trust escalation

#### Scenario: Receive a response with stale commentary
- **WHEN** an MCP response includes overlay commentary whose provenance revision predates the current graph revision
- **THEN** the response labels the commentary field with its staleness state
- **AND** the structural content is presented without modification regardless of commentary freshness

#### Scenario: Receive a response where commentary was budget-withheld
- **WHEN** a card request does not include commentary because including it would exceed the budget tier
- **THEN** the response marks the commentary field as budget-withheld
- **AND** the caller can distinguish budget-withheld from absent

#### Scenario: Receive a response from synrepo_docs_search
- **WHEN** `synrepo_docs_search` returns an explain commentary match
- **THEN** the response includes `commentary_state`, `generated_at`, `model_identity`, and `source_store: "overlay"`
- **AND** the result is clearly advisory and does not claim graph-backed truth

#### Scenario: Receive a response with fresh cross-link candidates
- **WHEN** an MCP response includes overlay cross-link candidates at `high` tier
- **THEN** the response labels each candidate with source store, freshness state, and confidence tier
- **AND** no candidate is presented as a graph-backed edge

#### Scenario: Receive a response where cross-links were budget-withheld
- **WHEN** a card request does not include proposed links because including them would exceed the budget tier
- **THEN** the response marks the `proposed_links` field as budget-withheld (`links_state: "budget_withheld"`)
- **AND** the caller can distinguish budget-withheld from absent

### Requirement: Default to minimal truthful context
synrepo SHALL define minimal-context behavior as the default for MCP responses, with budget-controlled escalation for deeper inspection.

#### Scenario: First call on an unfamiliar codebase
- **WHEN** an agent requests project orientation without specifying a deep read
- **THEN** the MCP surface returns the smallest useful context first
- **AND** it provides a defined path to request more detail when needed

### Requirement: Define synrepo_findings as an operator-facing audit tool
synrepo SHALL expose a `synrepo_findings` MCP tool that returns overlay audit information not suitable for agent card responses. The tool SHALL return: (a) all `review_queue`-tier cross-link candidates with their endpoints, evidence-span counts, confidence scores, and freshness states; (b) all `below_threshold` candidates (retrievable only through this tool, never in cards); and (c) candidates whose endpoints were deleted (`source_deleted`). The tool SHALL accept an optional filter by node ID, overlay edge kind, or freshness state. Responses SHALL include full provenance for each returned candidate. The tool is operator-facing and MAY be omitted or return an error in `auto` mode deployments that disable the review surface.

#### Scenario: Operator enumerates the review queue
- **WHEN** an operator invokes `synrepo_findings` without filters
- **THEN** the tool returns all `review_queue`-tier candidates with full provenance and freshness state
- **AND** the response is bounded by a configurable pagination limit

#### Scenario: Operator inspects below-threshold candidates for a specific node
- **WHEN** an operator invokes `synrepo_findings` filtered by a node ID
- **THEN** the tool returns every candidate (any tier, including `below_threshold`) where the node appears as source or target
- **AND** the response includes the numeric confidence score alongside the tier so the operator can reason about threshold tuning

#### Scenario: Tool rejects invocation in auto mode when review is disabled
- **WHEN** `synrepo_findings` is invoked in a deployment configured to disable the review surface
- **THEN** the tool returns an explicit error indicating the audit surface is not available
- **AND** no candidate data is returned

### Requirement: Expose a bounded recent-activity surface
synrepo SHALL expose a `synrepo_recent_activity(scope?, kinds?, limit?, since?)` MCP tool that returns a bounded lane over synrepo's own operational history. The tool SHALL surface: (a) recent reconcile outcomes with timestamp, file-count delta, duration, and success/failure; (b) recent repair-log entries (drift surface, severity, action taken) read from `.synrepo/state/repair-log.jsonl`; (c) recent cross-link accept/reject decisions from the overlay; (d) recent commentary refreshes with content-hash freshness state; (e) recent churn-hot files derived from persisted Git intelligence. The `kinds` parameter SHALL filter to any subset of `reconcile | repair | cross_link | overlay_refresh | hotspot`. The tool SHALL cap results (default 20, maximum 200) and SHALL NOT accept unbounded lookback (either `limit` or `since` SHALL bound the response). The tool is explicitly NOT a session-memory log, NOT an agent-interaction history, and NOT a replacement for `git log`; it surfaces synrepo's own operational events only. The tool SHALL NOT record caller identity, prompt content, or agent-facing interactions.

#### Scenario: Agent requests recent reconcile outcomes
- **WHEN** an agent invokes `synrepo_recent_activity` with `kinds: ["reconcile"]` and `limit: 10`
- **THEN** the tool returns the most recent reconcile events with timestamp, file-count delta, duration, and success/failure
- **AND** no other activity kinds are included

#### Scenario: Agent filters by multiple activity kinds
- **WHEN** an agent invokes `synrepo_recent_activity` with `kinds: ["repair", "cross_link"]`
- **THEN** the response contains only repair-log entries and cross-link accept/reject events
- **AND** each entry is labeled with its kind and source store

#### Scenario: Tool refuses unbounded lookback
- **WHEN** `synrepo_recent_activity` is invoked without a `limit` or `since` argument
- **THEN** the tool applies the default cap (20 entries)
- **AND** responses exceeding the hard maximum (200 entries) SHALL be rejected with an explicit error rather than silently truncated

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_recent_activity` appears in the tool list
- **AND** the tool description indicates it returns a bounded lane of synrepo operational events

#### Scenario: Tool is not a session-memory or agent-history surface
- **WHEN** `synrepo_recent_activity` is invoked
- **THEN** the response contains only synrepo's own operational events (reconcile, repair, cross-link, overlay, hotspot)
- **AND** no agent identity, prompt content, or inter-session interaction data appears in any response field

### Requirement: Define synrepo_change_risk as an on-demand risk assessment tool
synrepo SHALL expose a `synrepo_change_risk(target?, budget?)` MCP tool that returns a `ChangeRiskCard` for the specified target (file path or symbol name). The `target` parameter SHALL be required and accept a file path (e.g., "src/lib.rs") or a qualified symbol name (e.g., "synrepo::config::Config"). The `budget` parameter SHALL accept `"tiny"` (default), `"normal"`, or `"deep"` and affects which risk signals are computed (see cards spec for budget tier behavior). The tool SHALL return a JSON object containing fields: `target_name`, `target_kind`, `risk_level`, `risk_score`, and `risk_factors`.

#### Scenario: Analyst requests risk assessment for a file
- **WHEN** an analyst invokes `synrepo_change_risk` with `target: "src/lib.rs"` and `budget: "normal"`
- **THEN** the tool returns a ChangeRiskCard with target kind "file"
- **AND** includes drift score and co-change partner signals

#### Scenario: Analyst requests risk assessment for a symbol
- **WHEN** an analyst invokes `synrepo_change_risk` with `target: "synrepo::bootstrap::bootstrap"`
- **THEN** the tool returns a ChangeRiskCard with target kind "symbol"
- **AND** includes drift score and co-change partner signals

#### Scenario: Missing target returns error
- **WHEN** `synrepo_change_risk` is invoked with a non-existent target
- **THEN** an error is returned indicating "target not found"

#### Scenario: Tool appears in MCP tool list
- **WHEN** an MCP client connects and retrieves the tool list
- **THEN** `synrepo_change_risk` appears in the available tools

### Requirement: Expose synrepo_context_pack as a batched read-only context tool
synrepo SHALL expose `synrepo_context_pack(goal?, targets?, budget?, budget_tokens?, include_tests?, include_notes?, limit?)` as an MCP tool that batches read-only context artifacts into one response. Each target SHALL be a structured object `{ kind, target, budget? }`, where `kind` is one of `file`, `symbol`, `directory`, `minimum_context`, `test_surface`, `call_path`, or `search`; raw string targets SHALL NOT be treated as the public schema. The response SHALL include `schema_version`, `context_state`, `artifacts`, `omitted`, and `totals`. Each successful artifact SHALL include `artifact_type`, `target`, `status`, `content`, and `context_accounting`. Failed artifacts SHALL use `artifact_type: "error"` and include a typed `error` object with code, message, and retryability. The tool SHALL NOT mutate repository files, overlays, or external process state except for existing best-effort context metrics.

#### Scenario: Batch file and symbol context
- **WHEN** an agent invokes `synrepo_context_pack` with file and symbol targets
- **THEN** the response includes artifacts in request order
- **AND** the response includes a `context_state` with `graph_epoch`, `repo_root`, `source_hashes`, token estimates, and truncation state

#### Scenario: Numeric budget omits lower-ranked artifacts
- **WHEN** an agent invokes `synrepo_context_pack` with `budget_tokens` lower than the full response estimate
- **THEN** synrepo keeps the first artifact, records omitted later artifacts under `omitted`, and sets `context_state.truncation_applied` to true

#### Scenario: Missing target produces typed error artifact
- **WHEN** an agent invokes `synrepo_context_pack` with a missing file target
- **THEN** the corresponding artifact has `artifact_type: "error"`
- **AND** the artifact includes a typed `error.code` and `error.message`
- **AND** other requested artifacts can still be returned in order

### Requirement: Expose read-only MCP resource templates for context artifacts
synrepo SHALL advertise read-only MCP resource templates for `synrepo://card/{target}`, `synrepo://file/{path}/outline`, `synrepo://context-pack?goal={goal}`, `synrepo://project/{project_id}/card/{target}`, `synrepo://project/{project_id}/file/{path}/outline`, and `synrepo://project/{project_id}/context-pack?goal={goal}`. Default resource reads SHALL return JSON content equivalent to the corresponding tool-backed context path for the server default repository. Project-qualified resource reads SHALL resolve `project_id` through the managed-project registry before returning equivalent context for that project. Resource reads SHALL NOT add mutation capability.

#### Scenario: List resource templates
- **WHEN** an MCP client lists resource templates
- **THEN** the response includes card, file outline, and context-pack URI templates

#### Scenario: Read file outline resource
- **WHEN** an MCP client reads `synrepo://file/src/lib.rs/outline`
- **THEN** synrepo returns JSON containing a `file_outline` artifact and a `context_state`

#### Scenario: Global resource reads require a default repository
- **WHEN** a global/defaultless MCP server has no session default repository
- **AND** a client reads `synrepo://card/src/lib.rs`
- **THEN** the resource read returns a clear not-found or initialization error instead of selecting an arbitrary project

#### Scenario: Read project-qualified resource
- **WHEN** an MCP client reads `synrepo://project/proj_1234/file/src/lib.rs/outline`
- **THEN** synrepo verifies `proj_1234` against the managed-project registry
- **AND** returns the file outline for that managed project

### Requirement: Expose synrepo_entrypoints as a task-first MCP tool
synrepo SHALL expose a `synrepo_entrypoints(scope?, budget?)` MCP tool that returns an `EntryPointCard` for the requested scope. The `scope` parameter SHALL be an optional path prefix string; when absent, the compiler scans all indexed files. The `budget` parameter SHALL accept `"tiny"` (default), `"normal"`, or `"deep"`. Results SHALL be sorted by kind (binary first, then cli_command, http_handler, lib_root) then by file path within each kind. The result set SHALL be limited to 20 entries by default. The tool SHALL return a parseable JSON object and SHALL NOT raise an error when no entry points are found — it returns an empty `entry_points` list instead.

#### Scenario: Agent requests entry points with no scope
- **WHEN** an agent invokes `synrepo_entrypoints` without a `scope` argument
- **THEN** the tool returns an `EntryPointCard` covering all indexed files
- **AND** results are sorted binary-first then by file path
- **AND** the result count is at most 20

#### Scenario: Agent requests entry points scoped to a directory
- **WHEN** an agent invokes `synrepo_entrypoints` with `scope: "src/bin/"`
- **THEN** only entry points whose file paths start with `src/bin/` are returned
- **AND** entry points from other directories are excluded

#### Scenario: No entry points found in scope
- **WHEN** `synrepo_entrypoints` is called with a `scope` that has no matching entry points
- **THEN** the tool returns a JSON object with an empty `entry_points` array
- **AND** no error or non-zero exit status is produced

#### Scenario: Tool respects budget parameter
- **WHEN** `synrepo_entrypoints` is called with `budget: "normal"`
- **THEN** each entry in the response includes the caller count and truncated doc comment
- **AND** source bodies are omitted

### Requirement: Expose synrepo_refactor_suggestions as a task-first advisory tool
synrepo SHALL expose `synrepo_refactor_suggestions(repo_root?, min_lines?, limit?, path_filter?)` as a read-only MCP tool that returns non-test source files whose physical line count is greater than the threshold. Defaults SHALL be `min_lines = 300` and `limit = 20`. The response SHALL include `source_store: "graph+filesystem"`, `metric: "physical_lines"`, `threshold`, `candidate_count`, `omitted_count`, `groups`, and `candidates`. Each candidate SHALL include graph-backed file identity and language metadata plus filesystem-backed line count, symbol counts, deterministic modularity tags, a short deterministic suggestion, and recommended follow-up MCP tools.

#### Scenario: Agent asks whether files should be refactored
- **WHEN** an agent invokes `synrepo_refactor_suggestions` without optional arguments
- **THEN** synrepo returns non-test source files over 300 physical lines
- **AND** candidates are sorted by descending line count, then path
- **AND** the tool does not write source files, graph rows, overlay rows, or external process state

#### Scenario: Agent scopes refactor suggestions
- **WHEN** an agent invokes `synrepo_refactor_suggestions` with `path_filter: "src/tui/"`
- **THEN** candidates outside that path prefix are excluded
- **AND** the response preserves the same JSON contract

#### Scenario: Test files are excluded
- **WHEN** a repository contains large files under `tests/`, `__tests__/`, `tests.rs`, `test_*`, `*_test.*`, `*.test.*`, or `*.spec.*`
- **THEN** those files are excluded from refactor suggestion candidates

### Requirement: Expose synrepo_minimum_context as a task-first MCP tool
synrepo SHALL expose `synrepo_minimum_context` as a task-first MCP tool that returns a budget-bounded 1-hop neighborhood around a focal symbol or file. The tool SHALL accept parameters: `target` (node ID or qualified path, required), `budget` (`tiny`, `normal`, `deep`, default `normal`). The response SHALL follow the minimum-context spec contract: focal card, structural neighbor summaries or full cards depending on budget, governing decisions, and co-change partners.

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_minimum_context` appears in the tool list alongside the existing task-first tools
- **AND** the tool description indicates it returns a budget-bounded neighborhood for a focal node

#### Scenario: Default budget is normal
- **WHEN** an agent invokes `synrepo_minimum_context` without specifying a budget
- **THEN** the tool uses `normal` budget as the default
- **AND** the response includes structural neighbor summaries and top-3 co-change partners

### Requirement: Expose synrepo_public_api as a directory API surface tool
synrepo SHALL expose `synrepo_public_api(path, budget?)` as an MCP tool that returns a `PublicAPICard` for the given directory path. The tool SHALL accept parameters: `path` (directory path, required), `budget` (`tiny`, `normal`, `deep`, default `tiny`). In v1, visibility detection is Rust-specific; non-Rust directories return an empty symbol list.

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_public_api` appears in the tool list alongside the other card-surface tools
- **AND** the tool description indicates it returns a `PublicAPICard` with public symbols and entry points

#### Scenario: Default budget is tiny
- **WHEN** an agent invokes `synrepo_public_api` without specifying a budget
- **THEN** the tool uses `tiny` budget as the default
- **AND** the response includes only `path`, `public_symbol_count`, `approx_tokens`, and `source_store`

### Requirement: Card-returning MCP tool descriptions name the escalation default
synrepo SHALL include a single, consistent escalation-default sentence in the `description` field of every card-returning MCP tool (`synrepo_card`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_entrypoints`, `synrepo_where_to_edit`, `synrepo_change_impact`). The sentence SHALL be sourced from a shared compile-time constant tied to the canonical agent-doctrine block so wording cannot drift per tool.

#### Scenario: Agent enumerates tools
- **WHEN** an MCP client connects and retrieves the tool list
- **THEN** every card-returning tool's description ends with the same escalation-default sentence
- **AND** non-card tools (`synrepo_search`, `synrepo_findings`, `synrepo_recent_activity`, `synrepo_overview`) do not include the escalation-default sentence because their default-budget semantics differ

#### Scenario: Shared constant prevents drift
- **WHEN** a contributor edits the escalation sentence in one tool description directly
- **THEN** the compiled tool descriptions diverge from the shared constant
- **AND** the shims test or a dedicated MCP description test fails, blocking the change

### Requirement: Expose synrepo_call_path as a call-path tracing tool
synrepo SHALL expose `synrepo_call_path(target, budget?)` as an MCP tool that returns a `CallPathCard` tracing execution paths from entry points to a target symbol. The tool SHALL accept parameters: `target` (symbol node ID or qualified name, required), `budget` (`tiny`, `normal`, `deep`, default `tiny`). Uses backward BFS over `Calls` edges with depth budget (8 at tiny/normal, 12 at deep).

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_call_path` appears in the tool list

#### Scenario: Return paths from entry points to target
- **WHEN** an agent invokes `synrepo_call_path` with a target symbol that has callers
- **THEN** the response includes a `CallPathCard` with paths from entry points to the target

### Requirement: Expose synrepo_test_surface as a test discovery tool
synrepo SHALL expose `synrepo_test_surface(scope, budget?)` as an MCP tool that returns a `TestSurfaceCard` discovering test functions related to a scope. The tool SHALL accept parameters: `scope` (file path or directory, required), `budget` (`tiny`, `normal`, `deep`, default `tiny`). Uses path-convention heuristics to associate test files with source files.

#### Scenario: Tool registration appears in MCP capabilities
- **WHEN** an MCP client connects and enumerates available tools
- **THEN** `synrepo_test_surface` appears in the tool list

#### Scenario: Return test entries for a scope
- **WHEN** an agent invokes `synrepo_test_surface` with a file path that has associated tests
- **THEN** the response includes a `TestSurfaceCard` with `TestEntry` records

### Requirement: Expose workflow aliases
synrepo SHALL expose MCP workflow aliases for orienting, finding, explaining, impact inspection, risk shorthand, test discovery, and changed-context review. The `synrepo_risks` alias SHALL return the same bounded context as `synrepo_impact` so agents who follow the CLI doctrine find a matching MCP tool.

#### Scenario: Agent follows the workflow aliases
- **WHEN** an agent calls `synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_risks`, `synrepo_tests`, or `synrepo_changed`
- **THEN** each alias returns bounded graph-backed or explicitly labeled overlay-backed context
- **AND** existing MCP tools remain available unchanged

#### Scenario: Agent calls synrepo_risks and synrepo_impact with the same target
- **WHEN** an agent invokes `synrepo_risks` and `synrepo_impact` with identical `target` and `budget` values on a stable repository state
- **THEN** both tools return byte-identical content
- **AND** both tools share the same accounting metadata

### Requirement: Accept optional numeric caps on card aliases
MCP card and workflow aliases SHALL accept `budget_tokens` where the response contains a set of cards.

#### Scenario: Agent supplies budget_tokens
- **WHEN** an agent invokes a card-set MCP alias with `budget_tokens`
- **THEN** synrepo treats that value as a hard response ceiling where the tool can safely truncate ranked results
- **AND** the returned accounting metadata reports whether truncation occurred

### Requirement: Expose workflow guidance in MCP descriptions
The MCP server SHALL expose concise workflow guidance in server info and relevant task-first tool descriptions.

#### Scenario: MCP client lists tools
- **WHEN** an MCP client enumerates synrepo tools
- **THEN** task-first tools such as orient, find, explain, impact, risks, tests, changed, and minimum-context include concise guidance about bounded context and escalation
- **AND** descriptions remain short enough to avoid bloating tool-list responses

#### Scenario: MCP server info is requested
- **WHEN** a client requests synrepo server info or instructions
- **THEN** the response names the orient, find, impact or risks, edit, tests, changed workflow
- **AND** it tells agents to read full files only after card routing or explicit insufficiency

### Requirement: Gate MCP mutation behind explicit process invocation
The MCP server SHALL remain read-only by default. Overlay-writing MCP tools SHALL be registered only when the server is started with `synrepo mcp --allow-overlay-writes`. Source-edit MCP tools SHALL be registered only when the server is started with `synrepo mcp --allow-source-edits`. Configuration MAY further restrict mutation capability, but configuration alone SHALL NOT enable mutating MCP tools.

#### Scenario: Default MCP does not advertise mutating tools
- **WHEN** a user starts `synrepo mcp` without mutation flags
- **AND** an MCP client lists tools
- **THEN** `synrepo_prepare_edit_context` is absent
- **AND** `synrepo_apply_anchor_edits` is absent
- **AND** `synrepo_refresh_commentary` is absent
- **AND** overlay note mutation tools are absent
- **AND** existing read-first tools remain available

#### Scenario: Overlay-write MCP advertises overlay mutation tools
- **WHEN** a user starts `synrepo mcp --allow-overlay-writes`
- **AND** an MCP client lists tools
- **THEN** `synrepo_refresh_commentary` is present
- **AND** overlay note mutation tools are present
- **AND** source edit tools remain absent unless source edits are also enabled

#### Scenario: Source-edit MCP advertises edit tools
- **WHEN** a user starts `synrepo mcp --allow-source-edits`
- **AND** policy does not further disable editing
- **AND** an MCP client lists tools
- **THEN** `synrepo_prepare_edit_context` is present
- **AND** `synrepo_apply_anchor_edits` is present
- **AND** each tool description states that it can lead to source file mutation only through the apply tool

#### Scenario: Config cannot silently enable edits
- **WHEN** repository or user configuration permits edit-capable MCP behavior
- **AND** the server is started as `synrepo mcp` without mutation flags
- **THEN** mutating tools are not registered
- **AND** calling a mutating tool by name returns a not-available error

### Requirement: Expose a prepare/apply anchored edit workflow
When edit mode is enabled, synrepo SHALL expose a two-step MCP workflow: `synrepo_prepare_edit_context` for preparing anchored source context and `synrepo_apply_anchor_edits` for validated source mutation. The apply tool SHALL require freshness inputs produced by prepare, including `task_id`, `anchor_state_version`, `path`, `content_hash`, `anchor`, optional `end_anchor`, `edit_type`, and `text`.

#### Scenario: Agent prepares and applies a single-file edit
- **WHEN** edit mode is enabled
- **AND** an agent calls `synrepo_prepare_edit_context` for a file target
- **THEN** the response includes a task ID, anchor state version, path, content hash, and prepared anchors
- **WHEN** the agent calls `synrepo_apply_anchor_edits` with those freshness fields and replacement text
- **THEN** synrepo validates the anchors against current file content before writing
- **AND** the response reports the per-file edit outcome and post-edit diagnostics

#### Scenario: Apply without prepare is rejected
- **WHEN** edit mode is enabled
- **AND** an agent calls `synrepo_apply_anchor_edits` with an unknown `task_id` or `anchor_state_version`
- **THEN** synrepo rejects the edit as stale or unprepared
- **AND** no source file is modified

#### Scenario: Command execution remains unavailable
- **WHEN** edit mode is enabled
- **AND** an MCP client lists tools
- **THEN** no arbitrary command execution tool is registered as part of this workflow

### Requirement: Resolve MCP repository state explicitly
The MCP server SHALL resolve repository state from an optional default repository and an optional per-tool `repo_root` parameter. When `repo_root` is provided, the server SHALL canonicalize it, require that it is either the default repository or a registered managed project, prepare state for that repository, and return errors without falling back to another repository.

#### Scenario: Repo-bound MCP call omits repo_root
- **WHEN** the MCP server was started with a usable default repository and a repo-addressable tool omits `repo_root`
- **THEN** the tool uses the default repository state

#### Scenario: Global MCP call supplies registered repo_root
- **WHEN** a repo-addressable tool is called with `repo_root = "/work/app"` and `/work/app` is registered
- **THEN** the tool resolves and uses `/work/app` repository state

#### Scenario: Global MCP call omits repo_root with no default
- **WHEN** the MCP server has no usable default repository and a repo-addressable tool omits `repo_root`
- **THEN** the tool returns an explicit error that `repo_root` is required
- **AND** no other repository state is used

#### Scenario: Tool supplies unregistered repo_root
- **WHEN** a repo-addressable tool is called with a path that is not the default repository and is not registered
- **THEN** the tool returns an error explaining that the repository is not managed by synrepo
- **AND** the error names `synrepo project add <path>` as the remedy

#### Scenario: Requested repository cannot be prepared
- **WHEN** a requested registered repository is uninitialized, partial, or store-incompatible
- **THEN** the tool returns the preparation error for that repository
- **AND** the server does not fall back to the default repository

### Requirement: Allow MCP startup without a default repository
`synrepo mcp` SHALL be able to start from a non-repository working directory when it is intended to serve registered projects by explicit `repo_root`. Startup without a default repository SHALL NOT make any repository-addressable tool succeed unless the tool call supplies a resolvable `repo_root`.

#### Scenario: Global agent launches MCP from home directory
- **WHEN** an agent launches `synrepo mcp` from a directory that is not initialized with synrepo
- **THEN** the MCP server starts in defaultless mode
- **AND** repository data is served only after a tool call supplies a registered `repo_root`

#### Scenario: Explicit repo override is invalid
- **WHEN** the user launches `synrepo mcp --repo /work/app` and `/work/app` cannot be prepared
- **THEN** startup fails with the repository preparation error
- **AND** defaultless mode is not used to hide the explicit invalid override

### Requirement: MCP resolution is global-lazy and does not start watch
The MCP server SHALL resolve repository state lazily from the default repository or a per-tool `repo_root`. Resolving a repository through MCP SHALL NOT start `synrepo watch`, install Git hooks, scan unrelated repositories, or launch any long-lived background process. Freshness remains explicit: operators use `synrepo watch`, `synrepo watch --daemon`, `synrepo install-hooks`, or reconciliation commands outside the MCP request path.

#### Scenario: Default repository MCP startup leaves watch inactive
- **WHEN** a user starts `synrepo mcp --repo /work/app`
- **AND** `/work/app` has no active watch service
- **THEN** MCP startup prepares repository state for reads
- **AND** it does not start a watch daemon

#### Scenario: Global MCP call resolves a registered repository lazily
- **WHEN** a global MCP tool call supplies `repo_root = "/work/app"`
- **AND** `/work/app` is a registered project with no active watch service
- **THEN** the tool resolves `/work/app` for that request
- **AND** it does not start a watch daemon

### Requirement: Accept repo_root on repo-addressable MCP tools
Every MCP tool that reads or mutates repository-scoped synrepo state SHALL accept an optional `repo_root` parameter unless it is explicitly documented as server-default-only. Repo-addressable tools include card lookup, search, docs search, context pack, graph primitives, where-to-edit, impact/risk, entrypoints, notes, module/public API cards, workflow aliases, findings, recent activity, and edit-capable tools.

#### Scenario: Graph primitive routes by repo_root
- **WHEN** an agent calls `synrepo_edges` with a valid node ID and `repo_root = "/work/app"`
- **THEN** the edge traversal runs against `/work/app`

#### Scenario: Workflow alias routes by repo_root
- **WHEN** an agent calls a workflow alias such as `synrepo_find` with `repo_root = "/work/app"`
- **THEN** the workflow result and any per-repo metrics are associated with `/work/app`

#### Scenario: Tool lacks repo_root support
- **WHEN** a repository-scoped MCP tool cannot accept `repo_root`
- **THEN** the tool description SHALL state that it only uses the server default repository
- **AND** it SHALL return a clear error when no default repository exists

### Requirement: MCP server registration is performed via the agent-config installer
The synrepo CLI SHALL register the `synrepo` MCP server in agent harness configurations exclusively through the `agent-config` installer (`McpSpec` + `mcp_by_id(<id>).install_mcp(<scope>, <spec>)`). The installed entry SHALL run the `synrepo` binary directly (no node, npx, uv, or wrapper indirection). For global scope the spec SHALL pass no `--repo` argument; for project scope the spec SHALL pass `--repo .` so the server resolves to the configured repository. The owner tag for every MCP install written by synrepo SHALL be the literal string `"synrepo"`.

#### Scenario: Global MCP install for Claude
- **WHEN** synrepo registers the MCP server globally for Claude
- **THEN** the installer writes an entry with `command = "synrepo"` and `args = ["mcp"]`
- **AND** the install is owned by tag `"synrepo"`

#### Scenario: Project-scoped MCP install for Codex
- **WHEN** synrepo registers the MCP server project-scoped for Codex
- **THEN** the installer writes an entry with `command = "synrepo"` and `args = ["mcp", "--repo", "."]` under `mcp_servers.synrepo`
- **AND** the install is owned by tag `"synrepo"`

### Requirement: Track MCP usage as operational counters only
For repository-scoped MCP calls that reach a prepared synrepo runtime, synrepo SHALL maintain best-effort context metrics for total MCP requests, per-tool calls, per-tool errors, resource reads, workflow alias calls, card accounting, and explicit advisory saved-context note mutations. These counters SHALL be exposed through `synrepo status --json`, human-readable status, and dashboard view models. Metrics SHALL NOT store prompt content, query strings, note claims, evidence bodies, caller identity, or session history.

#### Scenario: MCP tool call updates per-repo counters
- **WHEN** an agent invokes `synrepo_search` with `repo_root = "/work/app"`
- **THEN** `/work/app/.synrepo/state/context-metrics.json` records an MCP request and increments the `synrepo_search` tool-call counter
- **AND** the stored metrics do not contain the search query text

#### Scenario: Advisory note write is counted as saved context
- **WHEN** an agent successfully invokes `synrepo_note_add`
- **THEN** synrepo increments an explicit saved-context note-write counter
- **AND** the note claim text is stored only in the advisory overlay note, not in context metrics

#### Scenario: Repeated registration is idempotent
- **WHEN** synrepo registers the MCP server twice with the same scope and content
- **THEN** the second call reports `already_installed = true`
- **AND** no file content changes on disk

### Requirement: MCP install scope coverage tracks installer support
The set of harnesses for which `synrepo setup` automates MCP registration SHALL be derived at runtime from `agent_config::mcp_by_id(<id>).is_some()` and that integration's `supported_scopes()`. synrepo SHALL NOT maintain a parallel hand-coded list of "automated" vs "shim-only" tiers for MCP registration. Harnesses that the installer does not support for a given scope SHALL be reported to the operator with the recommended fallback (project-scoped install, manual configuration, or unsupported).

#### Scenario: New installer-supported harness becomes automated
- **WHEN** a new agent harness gains MCP support in the agent-config crate
- **THEN** updating the synrepo dependency surfaces that harness for `synrepo setup`
- **AND** no per-harness MCP writer is added to synrepo

#### Scenario: Installer reports an unsupported scope
- **WHEN** `synrepo setup` is invoked for a harness that supports only one scope
- **THEN** synrepo selects the supported scope or reports the limitation before writing anything
- **AND** the operator is shown how to override the default scope

### Requirement: Inline-secret refusal is surfaced as a setup error
If a future synrepo MCP spec ever supplies an environment value to the installer in a way the installer would refuse (for example `InlineSecretInLocalScope`), `synrepo setup` SHALL surface the refusal as a setup error with the offending key name and SHALL NOT bypass the installer's secret policy. synrepo's own MCP server takes no secrets today, so the default path SHALL pass no inline secrets; this requirement governs future extensions.

#### Scenario: Refused inline secret aborts setup
- **WHEN** an MCP install would write an inline secret refused by the installer
- **THEN** `synrepo setup` aborts with the integration ID and env-key name
- **AND** no partial config is left on disk

### Requirement: Expose filtered syntext search through MCP
The MCP server SHALL expose `synrepo_search` as an exact lexical search tool backed by the syntext substrate index. The tool SHALL accept `query`, optional `limit`, optional `path_filter`, optional `file_type`, optional `exclude_type`, and optional `case_insensitive`; it SHALL also accept `ignore_case` as an alias for `case_insensitive`. The tool SHALL preserve the existing `query` and `results` response fields, and each result SHALL include `path`, `line`, and `content`.

#### Scenario: Existing minimal search remains valid
- **WHEN** an agent invokes `synrepo_search` with only `query` and `limit`
- **THEN** the tool returns exact lexical matches from the syntext substrate index
- **AND** the response still includes `query` and `results`

#### Scenario: Agent scopes search with filters
- **WHEN** an agent invokes `synrepo_search` with `path_filter`, `file_type`, `exclude_type`, or `case_insensitive`
- **THEN** the tool applies those options through the syntext substrate search path
- **AND** the response contains only matching entries

#### Scenario: Agent inspects search provenance
- **WHEN** an agent invokes `synrepo_search`
- **THEN** the response includes `engine: "syntext"`, `source_store: "substrate_index"`, `limit`, `filters`, and `result_count`

### Requirement: Keep MCP search freshness explicit
The MCP server SHALL keep `synrepo_search` read-only. Search calls MUST NOT trigger reconcile, start watch, rebuild the index, or mutate repo-tracked files or synrepo runtime stores. Index freshness SHALL be maintained by explicit init, reconcile, sync, or watch flows.

#### Scenario: Agent searches after source changes
- **WHEN** an agent invokes `synrepo_search` after source files changed but before an explicit refresh path has run
- **THEN** the tool searches the currently persisted substrate index
- **AND** it does not run reconcile or update the index as part of the search call

### Requirement: Describe context-pack as current batched context delivery
The MCP surface SHALL describe `synrepo_context_pack` as the current read-only batching tool for known files, symbols, directories, tests, call paths, searches, and minimum-context artifacts. This description SHALL NOT rename the tool, change its target schema, or introduce `synrepo_ask`.

#### Scenario: Agent reads MCP docs
- **WHEN** an agent reads the MCP guide
- **THEN** `synrepo_context_pack` is described as the current batched card/context delivery surface
- **AND** the docs do not imply that a new high-level ask tool already exists

#### Scenario: MCP tools are listed
- **WHEN** an MCP client retrieves available tools
- **THEN** the tool set remains unchanged by this framing change
- **AND** existing request and response contracts remain unchanged

### Requirement: Expose synrepo_task_route as a read-only MCP tool
The MCP server SHALL expose `synrepo_task_route(task, path?)` as a read-only tool that classifies a plain-language task and returns deterministic routing recommendations. The response SHALL include `intent`, `confidence`, `recommended_tools`, `budget_tier`, `llm_required`, `edit_candidate`, `signals`, and `reason`.

#### Scenario: Route a context task
- **WHEN** an agent invokes `synrepo_task_route` with a broad search or codebase review task
- **THEN** the response recommends read-only context tools such as `synrepo_orient`, `synrepo_search` with compact output, `synrepo_find`, `synrepo_minimum_context`, `synrepo_risks`, or `synrepo_tests`
- **AND** the response marks LLM use as not required when structural context is sufficient

#### Scenario: Route an unsupported semantic transform
- **WHEN** an agent invokes `synrepo_task_route` with a task such as adding types, converting promises to async/await, or adding error handling
- **THEN** the response sets `llm_required = true`
- **AND** it does not report a deterministic edit candidate

#### Scenario: Route a conservative edit candidate
- **WHEN** an agent invokes `synrepo_task_route` with a supported mechanical edit intent
- **THEN** the response includes `edit_candidate.intent`
- **AND** the recommended tools still require `synrepo_prepare_edit_context` and `synrepo_apply_anchor_edits` for mutation

### Requirement: Keep task routing content-free and read-only
Task routing SHALL NOT run shell commands, reconcile the repository, start watch, refresh commentary, write source files, write overlay content, or persist the task text. Any metrics recorded for routing SHALL be aggregate counters only.

#### Scenario: Task route records metrics
- **WHEN** `synrepo_task_route` is invoked against a prepared repository
- **THEN** context metrics increment aggregate route counters
- **AND** the metrics do not store the task, path, prompt, source text, caller identity, or response body

### Requirement: Default task routing to semantic intent matching when local assets are available
`synrepo_task_route` SHALL use semantic intent matching by default when `semantic-triage` is compiled, `enable_semantic_triage = true`, and the configured model artifacts are already locally loadable. The response SHALL include `routing_strategy` and MAY include `semantic_score`. If semantic matching is unavailable, the response SHALL remain keyword-based and include `routing_strategy: "keyword_fallback"`.

#### Scenario: Semantic routing is unavailable
- **WHEN** the tool is invoked without compiled semantic support, with semantic config disabled, or without local model assets
- **THEN** it returns the keyword route
- **AND** the route includes `routing_strategy: "keyword_fallback"`
- **AND** no network download is attempted

#### Scenario: Deterministic safety takes precedence
- **WHEN** task text matches a deterministic unsupported-transform guard
- **THEN** the route remains `llm-required`
- **AND** semantic matching cannot downgrade it to a mechanical edit route

### Requirement: Default MCP search to hybrid retrieval when local semantic assets are available
`synrepo_search` SHALL accept `mode = "auto" | "lexical"` with default `auto`. Auto mode SHALL use hybrid lexical plus semantic retrieval when semantic triage is locally available, otherwise lexical search. Lexical mode SHALL preserve the previous syntext-only behavior. Results SHALL include a `source` label, `fusion_score`, optional `semantic_score`, and nullable `line` / `content` fields for semantic-only matches.

#### Scenario: Auto search falls back to lexical
- **WHEN** semantic triage is disabled or local vector/model assets cannot load
- **THEN** `synrepo_search` returns lexical results
- **AND** the response identifies the engine as lexical fallback rather than failing the request

#### Scenario: Hybrid search returns semantic-only rows
- **WHEN** a semantic match is not present in the lexical result set
- **THEN** the row may have `line = null` and `content = null`
- **AND** it includes `source: "semantic"` or `source: "hybrid"` plus scores that explain the ranking

### Requirement: Expose opt-in compact MCP read output
Search compact output is no longer only opt-in. `synrepo_search` SHALL default to compact output while preserving explicit `output_mode = "default"` for bounded raw rows.

#### Scenario: Default search returns compact output
- **WHEN** an agent invokes `synrepo_search` without `output_mode`
- **THEN** the response groups results by file path and returns short line previews instead of the full raw result array
- **AND** the response includes `suggested_card_targets` so the caller can escalate to cards for bounded detail

#### Scenario: Explicit default search remains compatible
- **WHEN** an agent invokes `synrepo_search` with `output_mode = "default"`
- **THEN** the response preserves bounded raw result rows
- **AND** each result still includes `path`, `line`, and `content` when available

#### Scenario: Compact search applies a token cap
- **WHEN** an agent invokes compact `synrepo_search` with `budget_tokens`
- **THEN** the response keeps ranked file groups in order until the cap is reached
- **AND** it reports omitted matches and `output_accounting.truncation_applied = true` when content was omitted

#### Scenario: Context pack compacts search artifacts
- **WHEN** an agent invokes `synrepo_context_pack` with `output_mode = "compact"` and includes a search target
- **THEN** search artifacts use the compact search representation
- **AND** card-shaped artifacts retain their existing `context_accounting`

### Requirement: Keep compact MCP output read-only
Compact MCP output SHALL NOT run shell commands, trigger reconcile, start watch, rebuild indexes, mutate repository files, or mutate graph or overlay stores. Compacting SHALL be deterministic and derived only from the normal read-tool response shape.

#### Scenario: Compact search reads the persisted index
- **WHEN** an agent invokes compact `synrepo_search`
- **THEN** the tool searches the currently persisted syntext substrate index
- **AND** it does not refresh or update the index as part of the call

#### Scenario: Compact output does not summarize with an LLM
- **WHEN** an agent invokes any compact MCP read output
- **THEN** synrepo computes previews, groups, estimates, and omissions deterministically
- **AND** no explain provider or LLM-backed commentary generator is called

### Requirement: Expose synrepo_ask as a bounded task-context MCP tool
synrepo SHALL expose `synrepo_ask(ask, scope?, shape?, ground?, budget?)` as a default read-only MCP tool. The tool SHALL compile the request into existing context-pack targets and return a JSON object containing `schema_version`, `ask`, `recipe`, `answer`, `cards_used`, `evidence`, `grounding`, `budget`, `omitted_context_notes`, `next_best_tools`, and `context_packet`.

#### Scenario: Agent asks for a scoped module review
- **WHEN** an agent invokes `synrepo_ask` with `ask: "review this module"` and `scope.paths: ["src/surface/mcp"]`
- **THEN** the response includes a bounded `context_packet`
- **AND** `cards_used` lists the rendered artifacts
- **AND** `next_best_tools` recommends drill-down tools

#### Scenario: Grounding requires citations
- **WHEN** an agent invokes `synrepo_ask` with `ground.mode = "required"` or `ground.citations = "required"`
- **THEN** the response includes `grounding.status`
- **AND** the response includes `evidence` entries for rendered artifacts when source fields are available
- **AND** each evidence entry includes `source_store`, `confidence`, and `provenance`
- **AND** each evidence entry includes `span` for the primary line span and `spans` for the full line-span list
- **AND** missing spans are represented as `null` rather than fabricated line ranges

#### Scenario: Tool remains read-only
- **WHEN** `synrepo_ask` is invoked
- **THEN** it SHALL NOT mutate source files, graph facts, overlay notes, commentary, or external process state
- **AND** it MAY update existing best-effort MCP/context metrics

### Requirement: Enforce MCP response budgets server-side
The MCP server SHALL apply a final deterministic token cap to every MCP tool response before returning it. The default soft cap SHALL be 4,000 estimated tokens and the hard cap SHALL be 12,000 estimated tokens. If a response exceeds the effective cap, the server SHALL prefer structured truncation of known large fields over raw string truncation and SHALL report truncation metadata.

#### Scenario: Oversized response is clamped
- **WHEN** an MCP read handler produces JSON above the effective response token cap
- **THEN** the server trims known large arrays or payload fields before returning
- **AND** the response includes `context_accounting.truncation_applied = true`
- **AND** the response includes `context_accounting.token_cap` and `context_accounting.truncation_reason`

#### Scenario: Tool error stays structured
- **WHEN** an MCP handler returns a structured error
- **THEN** the server preserves the `ok`, `error`, and `error_message` fields
- **AND** the response remains valid JSON

### Requirement: Default MCP routing surfaces to compact bounded output
MCP search and list-style read tools SHALL use small default limits, hard maximums, and bounded token budgets. `limit: 0` SHALL NOT mean unbounded; it SHALL clamp to `1`.

#### Scenario: Search defaults are compact and bounded
- **WHEN** an agent invokes `synrepo_search` without `output_mode`, `limit`, or `budget_tokens`
- **THEN** the effective output mode is `compact`
- **AND** the effective limit is `10`
- **AND** the effective token budget is `1500`

#### Scenario: Search raw output remains explicit
- **WHEN** an agent invokes `synrepo_search` with `output_mode = "default"`
- **THEN** raw match rows may be returned
- **AND** the result count remains bounded by the effective limit and final response cap

#### Scenario: Cards mode rejects broad requests
- **WHEN** an agent invokes `synrepo_search` with `output_mode = "cards"` and no narrowing filter
- **AND** the effective limit is greater than `5`
- **THEN** the tool returns `INVALID_PARAMETER` with guidance to use compact output or narrow the request

### Requirement: Bound card and context-pack escalation
MCP card and context-pack tools SHALL default to tiny budget, cap batch sizes, and reject broad deep escalation.

#### Scenario: Deep card batches are rejected
- **WHEN** an agent requests `synrepo_card` with `budget = "deep"` and more than 3 targets
- **THEN** the tool returns `INVALID_PARAMETER`
- **AND** no cards are returned

#### Scenario: Context pack requires a focal input
- **WHEN** an agent invokes `synrepo_context_pack` without targets and without a non-empty goal
- **THEN** the tool returns `INVALID_PARAMETER`

#### Scenario: Context pack omits lower-priority artifacts
- **WHEN** a context pack exceeds its effective token budget
- **THEN** focal artifacts are retained before tests, risks, notes, and search artifacts
- **AND** omitted artifacts include target and reason metadata

### Requirement: Bound raw graph primitives
Raw graph primitive MCP tools that can fan out SHALL accept bounded limits and report omissions.

#### Scenario: Graph query applies a default limit
- **WHEN** an agent invokes `synrepo_query` without a limit
- **THEN** the tool returns at most 100 edges
- **AND** omitted edge counts are reported when more results exist

#### Scenario: Graph query clamps zero limit
- **WHEN** an agent invokes `synrepo_edges` or `synrepo_query` with `limit = 0`
- **THEN** the effective limit is `1`
