## Why

synrepo already has the stronger code foundation: deterministic graph-backed cards, advisory overlays, and task-first MCP tools. What is missing is a visible context-saving contract that agents and users can verify through accounting, workflow aliases, metrics, and benchmarks.

## What Changes

- Add shared context-accounting metadata to card-shaped responses so every response explains its estimated token cost, raw-file comparison, source hashes, freshness, and truncation state.
- Add numeric context-budget caps alongside the existing `tiny` / `normal` / `deep` tiers for card-set entry points.
- Add CLI and MCP workflow aliases that teach the obvious loop: orient, find cards, inspect impact, edit, validate tests, and check changed context.
- Add a separate operational context metrics store under `.synrepo/state/` and surface it through status, stats, and the dashboard.
- Add a reproducible context benchmark that measures compression plus usefulness, not token reduction alone.
- Update README, generated agent doctrine, and `skill/SKILL.md` to market bounded structural cards instead of unproven percentage savings.
- Do not add generic agent memory CRUD in this change. Capture `overlay-agent-notes-v1` as follow-up work because it needs its own provenance, decay, invalidation, and trust UX.

## Capabilities

### New Capabilities
- `context-accounting`: Card response accounting, context metrics, workflow aliases, and context-savings benchmark behavior.

### Modified Capabilities
- `cards`: Card contracts gain shared accounting metadata and numeric cap semantics.
- `mcp-surface`: MCP gains workflow aliases and optional numeric caps while preserving existing tools.
- `dashboard`: Dashboard and status surfaces include context metrics from the shared status snapshot.
- `agent-doctrine`: Agent-facing doctrine teaches the orient-find-impact-edit-tests-changed loop.
- `evaluation`: Benchmark requirements become concrete enough to run from the CLI.

## Impact

- Runtime card types and compiler helpers under `src/surface/card/`.
- MCP tool params and aliases under `src/surface/mcp/` and `src/bin/cli_support/commands/mcp.rs`.
- CLI command declarations and dispatch under `src/bin/cli_support/` and `src/bin/cli.rs`.
- Operational status and dashboard view models under `src/surface/status_snapshot.rs` and `src/tui/`.
- Documentation in `README.md`, `skill/SKILL.md`, and `src/surface/agent_doctrine.rs`.
