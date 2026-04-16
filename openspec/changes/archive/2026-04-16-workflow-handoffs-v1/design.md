## Context

synrepo already surfaces repair recommendations via `synrepo check`, cross-link candidates via the overlay, and git hotspots via the graph. However, there is no unified surface that aggregates these signals into actionable items for a developer picking up work.

The handoffs surface reads existing data sources and derives prioritized action items at query time. This avoids duplicating data into yet another store.

## Goals / Non-Goals

**Goals:**
- Add `synrepo handoffs` CLI command that reads repair-log, overlay candidates, and git hotspots, then emits prioritized actionable items
- Add `synrepo_next_actions` MCP tool for AI agents to request handoffs on demand
- Keep handoffs derived at query time (no new persistent store)

**Non-Goals:**
- Not a notification system (no push, no polling)
- Not a task tracker (no mutable state, no assignment)
- Not a full handoff narrative (just actionable items, not prose)
- Does not read the full repair report; reads only repair-log for resolution history

## Decisions

1. **Derive handoffs at query time** — Repair-log, overlay candidates, and git hotspots already exist. Handoffs aggregates these signals without persisting new data. This matches the "smallest truthful context first" principle.

2. **Priority ordering** — Items are ordered by severity (repair severity > cross-link confidence > hotspot recency), then by affected surface (structural surfaces first, then overlay).

3. **Read sources**:
   - Repair-log: `.synrepo/state/repair-log.jsonl` — read last N entries, filter for unresolved items
   - Overlay: pending cross-link candidates with `status = pending`
   - Git hotspots: top N files by commit frequency in last 90 days (via existing git-intelligence query)

4. **Output format** — Markdown table for CLI, JSON for MCP. Both include: item type, source file/symbol, recommendation, priority.

## Risks / Trade-offs

- **Stale data** — Handoffs reads current state. If repair-log has old unresolved items that are no longer relevant, they may appear. Mitigation: filter by recency (configurable, default 30 days).
- **No persistence** — Each query recomputes. For large repos, this adds latency. Mitigation: cache handoffs in memory for the duration of a single CLI/MCP request (no cross-request caching).
- **Tool registration** — Need to register `synrepo_next_actions` in the MCP tool list. This requires updating `src/bin/cli_support/commands/mcp.rs` and the MCP surface spec.
