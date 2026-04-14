## Purpose
Define the task-first MCP tools and response contracts that expose card-based synrepo behavior to coding agents.
## Requirements
### Requirement: Provide task-first MCP tools
synrepo SHALL define an MCP surface centered on task-first tools for orientation, card lookup, where-to-edit, change impact, entrypoints, call paths, test surface, minimum context, and findings.

#### Scenario: Route an edit from task language
- **WHEN** an agent asks where to edit for a task description
- **THEN** the MCP surface defines a task-first tool that returns bounded card-based results
- **AND** the tool contract does not require raw graph traversal knowledge from the caller

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
