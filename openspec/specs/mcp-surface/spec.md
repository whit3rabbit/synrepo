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

