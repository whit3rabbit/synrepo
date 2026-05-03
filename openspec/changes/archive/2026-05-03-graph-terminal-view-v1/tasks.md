## 1. OpenSpec And Contracts

- [x] 1.1 Validate the OpenSpec proposal, design, tasks, and spec deltas for graph terminal view.
- [x] 1.2 Update durable docs and examples to use canonical `sym_...` IDs and mention `synrepo graph view`.

## 2. Graph Model

- [x] 2.1 Add legacy `symbol_...` input aliasing while preserving canonical `sym_...` output.
- [x] 2.2 Add a shared bounded graph-neighborhood model with target resolution, top-degree overview, direction/depth/limit handling, edge filters, counts, truncation, provenance, and epistemic labels.

## 3. MCP Surface

- [x] 3.1 Update MCP `synrepo_query` to resolve file paths and symbol names the same way CLI graph query does.
- [x] 3.2 Add MCP `synrepo_graph_neighborhood` registration, parameters, handler, docs, and tests.

## 4. CLI And TUI

- [x] 4.1 Split near-cap CLI graph argument/dispatch code so new graph commands stay under the 400-line limit.
- [x] 4.2 Add `synrepo graph view [target] [--direction] [--edge-kind] [--depth] [--limit] [--json]`.
- [x] 4.3 Add a focused Ratatui graph-view entrypoint and widget with deterministic layout, node list, graph pane, details pane, filtering, selection, refocus, direction, depth, and quit controls.

## 5. Verification

- [x] 5.1 Add unit tests for ID aliasing, graph model traversal, truncation, and deterministic rendering.
- [x] 5.2 Add MCP and CLI tests for query parity, graph-neighborhood responses, registration, clap parsing, JSON output, and TUI widget rendering.
- [x] 5.3 Run `cargo fmt --check`, `cargo test graph`, `cargo test mcp`, and `cargo test tui`; fix any failures.
