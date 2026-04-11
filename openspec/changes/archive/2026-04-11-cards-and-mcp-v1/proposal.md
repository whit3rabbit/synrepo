## Why

Synrepo has a populated graph (stages 1–3) but no agent-facing query surface beyond raw node/edge inspection. Cards are the primary product surface defined in the design docs: compact, budget-aware context packets that let coding agents orient, route edits, and assess impact without reading arbitrary files. Without cards and an MCP server, the graph is infrastructure with no consumer.

This change delivers the next functional milestone: agents can query synrepo over MCP and get structured context.

## What Changes

- Stage 4 cross-file edge resolution: emit `Calls` and `Imports` edges from the structural pipeline using name-based approximate resolution (tree-sitter query + post-parse name lookup pass).
- `CardCompiler` implementation backed by `SqliteGraphStore`, covering `SymbolCard`, `FileCard`, and `ModuleCard` at all three budget tiers.
- Cargo workspace conversion: add `[workspace]` to root `Cargo.toml`, create `crates/synrepo-mcp/` member with its own deps (`rmcp`, `tokio`).
- MCP server in `crates/synrepo-mcp/` exposing the five core tools: `synrepo_card`, `synrepo_search`, `synrepo_overview`, `synrepo_where_to_edit`, `synrepo_change_impact`.

## Capabilities

### New Capabilities
- `cards`: agents can request SymbolCard, FileCard, or ModuleCard at tiny/normal/deep budget
- `mcp-surface`: five task-first MCP tools accessible to any MCP-compatible agent host
- `cross-file-edges`: Calls and Imports edges populate the graph during structural compile

### Modified Capabilities
- `structural-pipeline`: stage 4 (cross-file edge resolution) is now wired; pipeline runs stages 1–4

## Impact

- Adds `rmcp` and `tokio` as deps only in `crates/synrepo-mcp/`, not in the library crate
- Stage 4 adds query overhead to structural compile proportional to file count; approximate, not exact type resolution
- Workspace conversion is backward-compatible: all existing library code stays at the repo root, only the binary moves to a new crate
- Does not change git mining (stage 5), overlay, drift scoring, or card commentary behavior
