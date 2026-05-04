## Why

Agents already save tokens when they follow synrepo's small-first workflow, but the fastest route is still implicit. Ruflo's Agent Booster pattern is useful because hook output names when an LLM is unnecessary, when compact context is enough, and when a deterministic edit path exists.

synrepo should copy that routing idea without copying Ruflo's broader orchestration scope. The product remains a code-context compiler: deterministic structural context first, optional LLM overlay second, and source mutation only through explicit edit-gated surfaces.

## What Changes

- Add a deterministic task-route classifier exposed through MCP and CLI.
- Extend Codex and Claude nudge hooks with structured fast-path signals.
- Record aggregate fast-path counters in context metrics.
- Surface those counters in status JSON, status text, and the dashboard Health tab.
- Document fast-path routing in MCP docs, skill guidance, and canonical doctrine.

## Capabilities

### New Capabilities

- `agent-fast-path-routing`: Deterministic task classification that recommends the cheapest safe synrepo route for context, LLM avoidance, and conservative edit candidates.

### Modified Capabilities

- `agent-doctrine`: Teach agents to look for fast-path signals before cold reads or LLM-heavy work.
- `mcp-surface`: Add the `synrepo_task_route` read-only tool.
- `anchored-edits`: Clarify deterministic edit candidates are recommendations only; writes still require prepared anchors and `synrepo_apply_anchor_edits`.
- `context-accounting`: Add content-free counters for route classifications, hook signals, edit candidates, anchored edit outcomes, and estimated LLM calls avoided.

## Impact

- Affected runtime paths: agent nudge hooks, MCP tool registration, CLI argument parsing and dispatch, context metrics, status snapshot/status rendering, and dashboard Health rows.
- Affected documentation: `docs/MCP.md`, `docs/FOUNDATION.md`, `skill/SKILL.md`, generated doctrine/shims, and capability specs.
- No storage schema migration, no broad source rewrite engine, no new agent memory, no federation, no swarm orchestration, and no automatic source mutation from hooks.
