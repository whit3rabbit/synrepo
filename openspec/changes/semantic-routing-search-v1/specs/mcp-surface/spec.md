## ADDED Requirements

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
