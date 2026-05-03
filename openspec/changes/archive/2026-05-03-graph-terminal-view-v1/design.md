## Context

synrepo already stores canonical graph nodes and active edges in SQLite, exposes raw traversal via CLI and MCP, and exports a self-contained HTML graph. The new feature should fill the gap between raw JSON and full browser export: a fast terminal neighborhood explorer that works during normal CLI use and a matching bounded response for MCP callers.

Important constraints:
- Graph facts remain canonical; runtime views and exports are convenience outputs.
- Multi-query graph reads must use a read snapshot.
- Existing Ratatui/crossterm dependencies should be reused.
- Several target files are near the 400-line cap, so new code must live in focused modules.

## Goals / Non-Goals

**Goals:**
- Provide `synrepo graph view` for bounded terminal graph exploration.
- Provide `synrepo graph view --json` and MCP `synrepo_graph_neighborhood` with the same shared model.
- Make MCP `synrepo_query` resolve file paths and symbol names like CLI `synrepo graph query`.
- Accept legacy `symbol_...` input while keeping `sym_...` as canonical output.

**Non-Goals:**
- No game engine, browser replacement, Dioxus TUI, or new UI framework.
- No storage schema changes or persisted layout state.
- No overlay promotion or use of generated views as explain input.
- No unbounded whole-graph interactive rendering in the terminal.

## Decisions

- Use a shared graph view model module instead of parsing exported `graph.json`. This avoids export/write-admission side effects and keeps CLI/MCP reads snapshot-scoped.
- Center the view on either a resolved target or top-degree overview. A missing target is an error; no target returns a deterministic top-degree graph slice.
- Keep traversal bounded with defaults `direction = both`, `depth = 1`, `limit = 100`, max depth `3`, and max limit `500`.
- Render terminal output through a dedicated `graph view` TUI entrypoint instead of adding another dashboard tab. The dashboard already has a compact seven-tab layout, while a focused command avoids keybinding and footer churn.
- Implement the terminal layout with Ratatui widgets and a Canvas-like center pane. The view can use deterministic radial positions rather than a physics engine so tests are stable.
- Split CLI graph command parsing into a separate module and keep MCP graph-neighborhood handling outside `primitives.rs` to preserve the line-cap invariant.

## Risks / Trade-offs

- Terminal graph drawing is lower fidelity than HTML. Mitigation: keep HTML export as the full visualization and make terminal view explicitly bounded.
- Large fanout nodes can still be visually dense. Mitigation: enforce limits, mark `truncated`, and show counts.
- Legacy `symbol_...` aliases could obscure canonical naming. Mitigation: accept aliases only on input and continue emitting `sym_...` everywhere.
- Duplicating graph query target resolution could drift. Mitigation: reuse the existing resolver path where possible and add tests for CLI/MCP parity.

## Migration Plan

- Add code and docs without changing stored data.
- Existing commands and MCP calls continue to work.
- Rollback is removing the new command/tool and alias parser support; no persisted state needs cleanup.
