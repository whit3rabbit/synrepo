# AGENTS.md

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

This directory contains the MCP server implementation.

## Skill guidance

Refer to `skill/SKILL.md` (repo root `skill/SKILL.md`) for tool descriptions, budget protocol, and usage patterns.

## Keep SKILL.md in sync

Tool registration lives in `src/bin/cli_support/commands/mcp/tools.rs` (search for `name = "synrepo_`). The dispatch entry point is `src/bin/cli_support/commands/mcp.rs`, with shared state in `mcp/state.rs`. Any change to the MCP surface that agents should see MUST be reflected in `skill/SKILL.md`:

- **New tool** intended for agent-facing use: add to `## Core tools` in `skill/SKILL.md`. If it is a low-level primitive (like `synrepo_node`, `synrepo_edges`), keep it out of `Core tools` but mention it in the overview text in `mcp/tools.rs` so discoverability is preserved.
- **Renamed or removed tool**: update `skill/SKILL.md` and any references in the overview string in `mcp/tools.rs`.
- **New budget tier or protocol change**: update `## Budget protocol` and `## Default path` in `skill/SKILL.md`.
- **Trust-model or freshness semantics change**: update `## Trust model` and `## Do not`.

The overview blurb in `mcp/tools.rs` (the `synrepo_overview` description) and `skill/SKILL.md` are the two things an agent sees first. They must tell the same story.

## Key files

- `mod.rs` — MCP protocol dispatch and tool registration
- `cards.rs` — card compiler integration
- `card_accounting.rs` — budget/accounting helpers for card compilation
- `context_pack.rs` — single-file context-pack handler
- `context_pack/` — context-pack assembly sub-module
- `edits/` — source-write MCP tool surface (only registered with `--allow-source-edits`)
- `search.rs` — lexical search handler
- `findings.rs` — audit and cross-link triage tools
- `audit.rs` — cross-link audit surface
- `notes.rs` — agent-notes overlay surface (read/write)
- `docs.rs` — synthesized commentary docs search handler
- `primitives.rs` — low-level MCP primitive tool handlers
- `helpers.rs` — shared utilities
- `snapshot_tests.rs` — `insta` snapshot coverage for tool output
- `README.md` — submodule notes

## Hard invariants

- Graph content is primary, overlay is advisory
- `synrepo_overview` is the first tool to call on unfamiliar repos
- Budget tiers: `tiny` → `normal` → `deep`
- Overlay promotion to graph only in curated mode
