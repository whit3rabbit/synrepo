# synrepo MCP server

Front-door product docs live in [`README.md`](../../../README.md) and [`docs/MCP.md`](../../../docs/MCP.md).
This file stays focused on the MCP surface exposed by `synrepo mcp`: handler layout, tool registration, and invariants.

## Run

```bash
cargo run -- mcp                    # stdio server for the current repo
cargo run -- mcp --repo <path>      # stdio server for a specific repo
cargo run -- mcp --allow-edits      # explicitly expose anchored edit tools
```

The server is a subcommand of the `synrepo` binary. There is no separate crate. Transport is stdio only.

MCP is read-first by default. Edit-capable tools are absent from `tools/list` unless the process was started with `synrepo mcp --allow-edits`. Repository or user configuration may further restrict editing later, but config alone must never enable mutating tools.

## Where things live

| Concern | Location |
|---------|----------|
| Tool registration, JSON schemas, dispatch | `src/bin/cli_support/commands/mcp.rs` |
| Per-request handler logic | `src/surface/mcp/*.rs` (this directory) |
| Shared state (`SynrepoState`, snapshot helpers) | `src/surface/mcp/mod.rs` |
| Public MCP guide | `docs/MCP.md` |
| Agent-facing protocol doc | `skill/SKILL.md` (repo root) |
| Spec | `openspec/specs/mcp-surface/spec.md` |

Keep `src/bin/cli_support/commands/mcp.rs` as the single registration file. The agent-doctrine source scan test (`src/bin/cli_support/agent_shims/tests.rs`) reads this path and will fail if tools are registered elsewhere.

## Tools

Current registrations (see `mcp.rs` for schemas):

**Workflow aliases:**
`synrepo_orient`, `synrepo_find`, `synrepo_explain`, `synrepo_impact`, `synrepo_risks`, `synrepo_tests`, `synrepo_changed`

**Task-first read tools:**
`synrepo_overview`, `synrepo_card`, `synrepo_context_pack`, `synrepo_search`, `synrepo_where_to_edit`, `synrepo_change_impact`, `synrepo_change_risk`, `synrepo_entrypoints`, `synrepo_test_surface`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_call_path`, `synrepo_next_actions`

`synrepo_search` is backed by the syntext substrate index and accepts `query`, optional `limit`, `path_filter`, `file_type`, `exclude_type`, and `case_insensitive` (`ignore_case` alias). It returns exact lexical results with syntext/source-store metadata and never mutates or refreshes the index during the search call.

**Advisory overlay and audit tools:**
`synrepo_docs_search`, `synrepo_refresh_commentary`, `synrepo_findings`, `synrepo_recent_activity`

**Advisory agent note tools:**
`synrepo_note_add`, `synrepo_note_link`, `synrepo_note_supersede`, `synrepo_note_forget`, `synrepo_note_verify`, `synrepo_notes`

**Edit-enabled only (`synrepo mcp --allow-edits`):**
`synrepo_prepare_edit_context`, `synrepo_apply_anchor_edits`

**Low-level primitives:**
`synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`, `synrepo_provenance`

**Resources:**
`synrepo://card/{target}`, `synrepo://file/{path}/outline`, `synrepo://context-pack?goal={goal}`

Resources are read-only mirrors of tool-backed context. Tool-only hosts should call `synrepo_context_pack`; resource-aware hosts can cache the URI forms.

## Edit-enabled workflow

Use read tools first. When edit mode is explicitly enabled, call `synrepo_prepare_edit_context` before `synrepo_apply_anchor_edits`.

`synrepo_prepare_edit_context` accepts file, symbol, and range targets. It returns compact source context plus `task_id`, `anchor_state_version`, `path`, `file_id`, `content_hash`, `source_hash`, and prepared line anchors.

`synrepo_apply_anchor_edits` validates the live anchor session, content hash, anchor existence, optional end-anchor ordering, and exact current boundary text before writing. File batches are atomic per file. Multi-file calls may return mixed per-file outcomes and never claim cross-file transaction semantics.

Prepared anchors are session-scoped operational cache entries. They are not graph nodes, overlay content, commentary, agent notes, canonical source truth, or agent memory. Reconcile remains the only producer of graph facts after a write.

Post-edit diagnostics are bounded to built-in validation, write status, reconcile or watch delegation, and test-surface recommendations. The edit tools do not register or run arbitrary command execution.

## Adding or changing a tool

1. Register in `src/bin/cli_support/commands/mcp.rs` (name, params struct, dispatch).
2. Add the handler in this directory, or extend an existing module (`cards.rs`, `search.rs`, `findings.rs`, `audit.rs`, `primitives.rs`, `helpers.rs`).
3. Update `skill/SKILL.md`:
   - Agent-facing tool: add to `## Core tools`.
   - Primitive: leave out of `Core tools` but mention in the `synrepo_overview` description string in `mcp.rs`.
4. Update `docs/MCP.md` when the public tool surface, resource list, workflow, trust model, or edit-gating behavior changes.
5. If the change alters budget tiers, freshness semantics, or trust model, update the corresponding `SKILL.md` sections.

The overview blurb in `mcp.rs`, `skill/SKILL.md`, and `docs/MCP.md` are the surfaces agents and operators see first. They must tell the same story.

## Invariants

- Graph content is primary; overlay is advisory.
- Edit tools require an explicit `--allow-edits` process gate and are hidden by default.
- Prepared anchors are session-scoped operational state, not canonical graph facts or agent memory.
- `synrepo_docs_search` returns advisory explained commentary only. It is searchable overlay output, not canonical graph state or explain input.
- Multi-query reads run under `with_graph_read_snapshot` / `with_overlay_read_snapshot`. The re-entrant depth counter lets handlers and card compilers nest snapshots safely (see hard invariant 8 in the root `AGENTS.md`).
- Overlay promotion to graph edges is curated-mode-only.
- Budget tiers: `tiny` → `normal` → `deep`. Default is `tiny`.

## Tests

Handler-level tests live alongside each module. End-to-end MCP request-response tests live under `src/bin/cli_support/tests/` and exercise the binary crate via the dispatch path in `src/bin/cli_support/commands/mcp.rs`.
