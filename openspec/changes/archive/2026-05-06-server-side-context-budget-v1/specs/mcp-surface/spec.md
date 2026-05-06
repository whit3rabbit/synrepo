## ADDED Requirements

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

## MODIFIED Requirements

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
