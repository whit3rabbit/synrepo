## Purpose
Define the contract for `synrepo_minimum_context`: a budget-bounded 1-hop neighborhood tool that returns the focal card, structural neighbors, governing decisions, and co-change partners in a single consistent response.

## Requirements

### Requirement: Define synrepo_minimum_context tool contract
synrepo SHALL expose a `synrepo_minimum_context` MCP tool that accepts a focal target (node ID or qualified path), a budget tier (`tiny`, `normal`, `deep`), and returns the focal card plus a budget-bounded 1-hop neighborhood. The response SHALL include: the focal card, structural neighbor summaries (outbound `Calls` and `Imports` edges), governing decisions (incoming `Governs` edges as DecisionCard summaries), and co-change partners from git intelligence.

#### Scenario: Agent requests minimum context for a symbol at normal budget
- **WHEN** an agent invokes `synrepo_minimum_context` with a symbol's node ID or qualified name at `normal` budget
- **THEN** the tool returns the focal `SymbolCard` at `normal` budget
- **AND** the response includes outbound `Calls` and `Imports` neighbors as summaries (node ID, qualified name, kind, edge type) capped at 10 per edge kind
- **AND** the response includes up to 3 co-change partners from the containing file's git-intelligence data, each labeled with `source: "git_intelligence"` and `granularity: "file"`
- **AND** the response includes incoming `Governs` edges as DecisionCard summaries

#### Scenario: Agent requests minimum context for a file at deep budget
- **WHEN** an agent invokes `synrepo_minimum_context` with a file path at `deep` budget
- **THEN** the tool returns the focal `FileCard` at `deep` budget
- **AND** the response includes full cards (not summaries) for outbound structural neighbors, capped at 10 per edge kind
- **AND** the response includes up to 5 co-change partners from the file's git-intelligence data
- **AND** the response includes full DecisionCards for incoming `Governs` edges

#### Scenario: Agent requests minimum context at tiny budget
- **WHEN** an agent invokes `synrepo_minimum_context` at `tiny` budget
- **THEN** the tool returns the focal card at `tiny` budget
- **AND** the response includes only edge counts (not neighbor details): `outbound_calls_count`, `outbound_imports_count`, `governs_count`, `co_change_count`
- **AND** no neighbor cards or co-change partner details are included

### Requirement: Neighborhood resolution runs under a single graph read snapshot
synrepo SHALL resolve all neighborhood data (focal card, outbound edges, inbound edges, governing concepts) under a single graph read snapshot so the response reflects a consistent epoch.

#### Scenario: Concurrent write does not split the neighborhood view
- **WHEN** a reconcile commit occurs during a `synrepo_minimum_context` invocation
- **THEN** the entire response (focal card and all neighbors) reflects the graph state at the start of the snapshot
- **AND** no part of the response reflects post-commit state

### Requirement: Co-change partners are sourced from git-intelligence cache
synrepo SHALL derive co-change partners from the per-file git-intelligence cache already maintained by the card compiler, not from graph edges. Co-change entries SHALL be labeled with `source: "git_intelligence"` and `granularity: "file"` so callers understand the precision boundary.

#### Scenario: Co-change partners are returned for a focal file
- **WHEN** `synrepo_minimum_context` is invoked for a file with git-intelligence data
- **THEN** the response includes co-change partners ranked by co-change count
- **AND** each partner entry includes the file path, co-change count, and the `source: "git_intelligence"` and `granularity: "file"` labels

#### Scenario: No co-change data available
- **WHEN** `synrepo_minimum_context` is invoked for a file in a repo with no git history or degraded history
- **THEN** the co-change partners list is empty
- **AND** the response includes `co_change_state: "missing"` rather than an absent field

### Requirement: Target resolution supports node IDs and qualified paths
synrepo SHALL accept the focal target as either a node ID string (e.g., `symbol_0000000000000024`, `file_0000000000000042`) or a qualified path (e.g., `src/surface/card/compiler/mod.rs::GraphCardCompiler::symbol_card`). The tool SHALL return an explicit error if the target does not resolve to an existing node.

#### Scenario: Resolve by node ID
- **WHEN** an agent passes a valid node ID string
- **THEN** the tool resolves the node and returns its neighborhood

#### Scenario: Resolve by qualified path
- **WHEN** an agent passes a qualified path that matches a symbol or file
- **THEN** the tool resolves the node using the existing `resolve_target` logic and returns its neighborhood

#### Scenario: Target does not exist
- **WHEN** an agent passes a target that does not match any node
- **THEN** the tool returns an error response indicating the target was not found
- **AND** the error includes the unresolved target string

### Requirement: No overlay content in minimum-context responses
synrepo_minimum_context SHALL NOT include overlay commentary, proposed links, or any overlay-sourced content in its response. The tool reads exclusively from the graph store and git-intelligence cache.

#### Scenario: Deep budget request does not include commentary
- **WHEN** `synrepo_minimum_context` is invoked at `deep` budget for a symbol that has overlay commentary
- **THEN** the focal card omits `overlay_commentary` and `proposed_links`
- **AND** the response does not include `commentary_state` or `links_state` labels
