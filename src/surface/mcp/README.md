# synrepo MCP server

Front-door product docs live in [`README.md`](../../../README.md).
This file stays focused on the MCP surface exposed by `synrepo mcp`: handler layout, tool registration, and invariants.

## Run

```bash
cargo run -- mcp                    # stdio server for the current repo
cargo run -- mcp --repo <path>      # stdio server for a specific repo
```

The server is a subcommand of the `synrepo` binary. There is no separate crate. Transport is stdio only.

## Where things live

| Concern | Location |
|---------|----------|
| Tool registration, JSON schemas, dispatch | `src/bin/cli_support/commands/mcp.rs` |
| Per-request handler logic | `src/surface/mcp/*.rs` (this directory) |
| Shared state (`SynrepoState`, snapshot helpers) | `src/surface/mcp/mod.rs` |
| Agent-facing protocol doc | `skill/SKILL.md` (repo root) |
| Spec | `openspec/specs/mcp-surface/spec.md` |

Keep `src/bin/cli_support/commands/mcp.rs` as the single registration file. The agent-doctrine source scan test (`src/bin/cli_support/agent_shims/tests.rs`) reads this path and will fail if tools are registered elsewhere.

## Tools

Current registrations (see `mcp.rs` for schemas):

**High-level / agent-facing:**
`synrepo_overview`, `synrepo_card`, `synrepo_context_pack`, `synrepo_search`, `synrepo_docs_search`, `synrepo_where_to_edit`, `synrepo_change_impact`, `synrepo_change_risk`, `synrepo_entrypoints`, `synrepo_test_surface`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_call_path`, `synrepo_refresh_commentary`, `synrepo_findings`, `synrepo_recent_activity`, `synrepo_next_actions`

**Low-level primitives:**
`synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`, `synrepo_provenance`

**Resources:**
`synrepo://card/{target}`, `synrepo://file/{path}/outline`, `synrepo://context-pack?goal={goal}`

Resources are read-only mirrors of tool-backed context. Tool-only hosts should call `synrepo_context_pack`; resource-aware hosts can cache the URI forms.

## Adding or changing a tool

1. Register in `src/bin/cli_support/commands/mcp.rs` (name, params struct, dispatch).
2. Add the handler in this directory, or extend an existing module (`cards.rs`, `search.rs`, `findings.rs`, `audit.rs`, `primitives.rs`, `helpers.rs`).
3. Update `skill/SKILL.md`:
   - Agent-facing tool: add to `## Core tools`.
   - Primitive: leave out of `Core tools` but mention in the `synrepo_overview` description string in `mcp.rs`.
4. If the change alters budget tiers, freshness semantics, or trust model, update the corresponding `SKILL.md` sections.

The overview blurb in `mcp.rs` and `skill/SKILL.md` are the two surfaces agents see first. They must tell the same story.

## Invariants

- Graph content is primary; overlay is advisory.
- `synrepo_docs_search` returns advisory explained commentary only. It is searchable overlay output, not canonical graph state or explain input.
- Multi-query reads run under `with_graph_read_snapshot` / `with_overlay_read_snapshot`. The re-entrant depth counter lets handlers and card compilers nest snapshots safely (see hard invariant 8 in the root `AGENTS.md`).
- Overlay promotion to graph edges is curated-mode-only.
- Budget tiers: `tiny` → `normal` → `deep`. Default is `tiny`.

## Tests

Handler-level tests live alongside each module. End-to-end MCP request-response tests live under `src/bin/cli_support/tests/` and exercise the binary crate via the dispatch path in `src/bin/cli_support/commands/mcp.rs`.
