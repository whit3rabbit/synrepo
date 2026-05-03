## ADDED Requirements

### Requirement: Expose opt-in compact MCP read output
The MCP server SHALL support opt-in compact output for read-only context tools without changing their default response shape. `synrepo_search` SHALL accept `output_mode` with values `"default"` and `"compact"` and an optional `budget_tokens`. `synrepo_context_pack` SHALL accept `output_mode` with values `"default"` and `"compact"` and SHALL reuse its existing `budget_tokens` cap.

#### Scenario: Default search remains compatible
- **WHEN** an agent invokes `synrepo_search` without `output_mode`
- **THEN** the response preserves the existing `query`, `results`, `engine`, `source_store`, `limit`, `filters`, and `result_count` fields
- **AND** each result still includes `path`, `line`, and `content`

#### Scenario: Compact search groups matches
- **WHEN** an agent invokes `synrepo_search` with `output_mode = "compact"`
- **THEN** the response groups results by file path and returns short line previews instead of the full raw result array
- **AND** the response includes `suggested_card_targets` so the caller can escalate to cards for bounded detail

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
