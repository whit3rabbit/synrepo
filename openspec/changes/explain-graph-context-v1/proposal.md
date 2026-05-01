## Why

Explain commentary currently receives uneven context depending on the entrypoint: repair sync uses richer graph/source context, while explicit refresh builds a much smaller symbol-card prompt. This makes generated commentary less reliable for connected code, especially imports, exports, callers, callees, and associated tests.

## What Changes

- Introduce a shared, token-aware explain context builder backed by canonical graph facts and source snippets.
- Use the shared builder for both repair/sync commentary generation and explicit `synrepo_refresh_commentary`.
- Include direct graph neighborhood facts (imports, imported-by files, callers, callees, visible/exported symbols, governing decisions, co-change partners, and associated tests) when budget allows.
- Trim lower-priority context before provider calls so `commentary_cost_limit` remains the single user-facing input budget guard.
- Preserve graph/overlay separation: overlay commentary, proposed links, and materialized explain docs are never used as explain input.

## Capabilities

### New Capabilities
- `explain-graph-context`: Defines the graph-backed, budget-bounded prompt context contract for commentary generation.

### Modified Capabilities
- None.

## Impact

- Affected runtime paths: `src/pipeline/explain`, `src/pipeline/repair/sync`, and `src/surface/card/compiler`.
- No CLI or MCP schema changes.
- No new dependencies, migrations, or storage schema changes.
