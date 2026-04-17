# AGENTS.md

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

This directory contains the MCP server implementation.

## Skill guidance

Refer to `skill/SKILL.md` (repo root `skill/SKILL.md`) for tool descriptions, budget protocol, and usage patterns.

## Keep SKILL.md in sync

Tool registration lives in `src/bin/cli_support/commands/mcp.rs` (search for `name = "synrepo_`). Any change to the MCP surface that agents should see MUST be reflected in `skill/SKILL.md`:

- **New tool** intended for agent-facing use: add to `## Core tools` in `skill/SKILL.md`. If it is a low-level primitive (like `synrepo_node`, `synrepo_edges`), keep it out of `Core tools` but mention it in the overview text in `mcp.rs` so discoverability is preserved.
- **Renamed or removed tool**: update `skill/SKILL.md` and any references in the overview string in `mcp.rs`.
- **New budget tier or protocol change**: update `## Budget protocol` and `## Default path` in `skill/SKILL.md`.
- **Trust-model or freshness semantics change**: update `## Trust model` and `## Do not`.

The overview blurb in `mcp.rs` (the `synrepo_overview` description) and `skill/SKILL.md` are the two things an agent sees first. They must tell the same story.

## Key files

- `mod.rs` — MCP protocol dispatch and tool registration
- `cards.rs` — card compiler integration
- `search.rs` — lexical search handler
- `findings.rs` — audit and cross-link triage tools
- `audit.rs` — cross-link audit surface
- `primitives.rs` — low-level MCP primitive tool handlers
- `helpers.rs` — shared utilities

## Hard invariants

- Graph content is primary, overlay is advisory
- `synrepo_overview` is the first tool to call on unfamiliar repos
- Budget tiers: `tiny` → `normal` → `deep`
- Overlay promotion to graph only in curated mode
