## Why

`context-accounting-and-workflow-v1` shipped the context-budget contract, the workflow aliases, the context metrics store, and the benchmark harness. Two small gaps remain that were observed while reviewing the rollout:

- The MCP surface exposes `synrepo_impact` but not a shorthand `synrepo_risks`, even though the CLI already ships a `risks` alias. Agents that read the CLI and MCP tool lists side-by-side see an inconsistency.
- The dashboard Health tab renders `cards served` and `avg tokens/card`, but the already-collected `estimated_tokens_saved_total` and `stale_responses_total` counters are not displayed. Operators cannot see context savings or advisory staleness at a glance, which defeats part of the "trust UX" goal of the accounting change.

Both gaps are small, additive, and backed by data that is already persisted in `.synrepo/state/context-metrics.json` via the shared status snapshot.

## What Changes

- Register `synrepo_risks` as an MCP workflow alias that mirrors `synrepo_impact`, keeping parity with the CLI `risks` alias.
- Update the MCP server `get_info` instruction string and the canonical agent doctrine to name `synrepo_risks` alongside `synrepo_impact`.
- Add two new rows to the dashboard Health tab: `tokens avoided` (from `estimated_tokens_saved_total`) and `stale responses` (from `stale_responses_total`, elevated to `Stale` severity when non-zero).
- Update specs in `mcp-surface`, `dashboard`, and `agent-doctrine` to cover the new alias and the required Health rows.
- No new data sources. No changes to the graph, overlay, or explain pipelines.

## Capabilities

### Modified Capabilities

- `mcp-surface`: workflow-alias requirement now names `synrepo_risks` and requires it to return the same bounded context as `synrepo_impact`.
- `dashboard`: context-metrics requirement names the specific Health rows that SHALL be rendered when metrics are present.
- `agent-doctrine`: generated doctrine SHALL name `synrepo_risks` next to `synrepo_impact`.

## Impact

- `src/bin/cli_support/commands/mcp.rs` — one additional `#[tool]` block, updated `get_info` instruction string.
- `src/tui/probe/mod.rs` — two additional Health rows read from `snapshot.context_metrics`.
- `src/surface/agent_doctrine.rs` — doctrine text names `synrepo_risks`.
- Spec deltas under `openspec/changes/context-workflow-follow-ups-v1/specs/`.
- No storage, migration, or dependency changes.
