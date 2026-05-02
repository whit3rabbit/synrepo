## ADDED Requirements

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
