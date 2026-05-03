## Why

The canonical graph is useful through CLI and MCP primitives, but users cannot quickly inspect a bounded neighborhood in the terminal. Agents also have a small ergonomics gap: MCP `synrepo_query` is stricter than the CLI even though both expose the same graph facts.

## What Changes

- Add a terminal graph runtime view for bounded graph neighborhoods using the existing Ratatui stack.
- Add a shared graph-neighborhood model that can back CLI JSON output, the TUI view, and MCP responses.
- Add MCP `synrepo_graph_neighborhood` for bounded graph-backed node/edge responses.
- Update MCP `synrepo_query` to resolve targets the same way CLI graph query does.
- Accept legacy `symbol_...` symbol IDs as input aliases while preserving canonical `sym_...` output.
- Update documentation and examples that still claim `symbol_...` is canonical.

## Capabilities

### New Capabilities
- `terminal-graph-view`: Defines the terminal runtime view and bounded graph-neighborhood response contract.

### Modified Capabilities
- `mcp-surface`: MCP graph primitives gain target-resolution parity and the graph-neighborhood tool.
- `exports-and-views`: Terminal graph view is classified as a runtime convenience view, not graph truth or explain input.

## Impact

- Affected runtime paths: CLI graph commands, MCP graph primitives, TUI rendering, graph ID parsing, and graph docs.
- No new dependencies, storage migrations, or graph schema changes.
- Existing `synrepo export --format graph-html` remains the richer browser visualization surface.
