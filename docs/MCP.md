# MCP

`synrepo mcp` serves repository context to MCP-compatible coding agents over stdio. It is the agent-facing surface for cards, search, impact analysis, test discovery, advisory overlay data, explicit saved-context notes, and optional anchored edits.

MCP is source-read-first by default. Source edit tools are hidden unless the server process starts with `synrepo mcp --allow-edits`. Advisory note tools can mutate only the overlay store and are explicit saved-context actions, not automatic session memory.

## Run

```bash
synrepo mcp                    # stdio server for the current repo
synrepo mcp --repo <path>      # stdio server for a specific repo
synrepo mcp --allow-edits      # explicitly expose anchored edit tools
```

Most users should prefer `synrepo setup <tool>`, which writes the agent instructions or skill and registers MCP through `agent-config` for supported integrations. The default is global agent config with `synrepo mcp`; pass `--project` to write repo-local MCP config that launches `synrepo mcp --repo .`. Global MCP is lazy: each tool call must supply a registered repository via `repo_root` unless the server has a default repository. Shim-only integrations still need their own MCP config pointed at `synrepo mcp --repo .`.

`synrepo mcp` does not start `synrepo watch`, install Git hooks, scan every repository, or keep state fresh in the background. Use `synrepo watch`, `synrepo watch --daemon`, or `synrepo install-hooks` explicitly when you want those behaviors.

## Default Agent Workflow

The default path is deliberately small first:

1. `synrepo_orient` before reading the repo cold.
2. `synrepo_find` or `synrepo_search` to route a task.
3. `synrepo_explain` for bounded details on a file or symbol.
4. `synrepo_minimum_context` when a focal target is known but surrounding risk is unclear.
5. `synrepo_impact` or `synrepo_risks` before edits.
6. `synrepo_tests` before claiming done.
7. `synrepo_changed` after edits to review changed context and validation commands.

Use `tiny` budgets to route, `normal` budgets to understand a neighborhood, and `deep` budgets only before implementation or when exact source details matter. Use `synrepo_context_pack` when batching several read-only context artifacts is cheaper than serial tool calls. Its `targets` parameter is an array of structured objects: `{ "kind": "file|symbol|directory|minimum_context|test_surface|call_path|search", "target": "...", "budget": "tiny|normal|deep" }`.

## Tool Groups

Workflow aliases:
- `synrepo_orient`
- `synrepo_find`
- `synrepo_explain`
- `synrepo_impact`
- `synrepo_risks`
- `synrepo_tests`
- `synrepo_changed`

Task-first read tools:
- `synrepo_overview`
- `synrepo_card`
- `synrepo_context_pack`
- `synrepo_search`
- `synrepo_where_to_edit`
- `synrepo_change_impact`
- `synrepo_change_risk`
- `synrepo_entrypoints`
- `synrepo_test_surface`
- `synrepo_module_card`
- `synrepo_public_api`
- `synrepo_minimum_context`
- `synrepo_call_path`
- `synrepo_next_actions`

Advisory overlay and audit tools:
- `synrepo_docs_search`
- `synrepo_refresh_commentary`
- `synrepo_findings`
- `synrepo_recent_activity`

Advisory agent note tools:
- `synrepo_note_add`
- `synrepo_note_link`
- `synrepo_note_supersede`
- `synrepo_note_forget`
- `synrepo_note_verify`
- `synrepo_notes`

Low-level graph primitives:
- `synrepo_node`
- `synrepo_edges`
- `synrepo_query`
- `synrepo_overlay`
- `synrepo_provenance`

Read-only resources:
- `synrepo://card/{target}`
- `synrepo://file/{path}/outline`
- `synrepo://context-pack?goal={goal}`

Edit-enabled tools, present only under `synrepo mcp --allow-edits`:
- `synrepo_prepare_edit_context`
- `synrepo_apply_anchor_edits`

## Trust Model

Graph-backed structural facts are authoritative. They come from parsers, Git, and human-declared inputs.

Overlay content is advisory. Commentary, explained docs, proposed cross-links, and agent notes are labeled as overlay-backed and freshness-sensitive. If graph facts and overlay prose disagree, trust the graph.

Freshness is explicit. A stale label is information, not an error, and synrepo does not silently refresh commentary just because an API key exists. Use `synrepo_refresh_commentary` only when fresh advisory prose is required.

Prepared edit anchors are short-lived operational state. They are not graph facts, overlay content, commentary, agent notes, canonical source truth, or agent memory.

Context metrics are operational counters only. They track totals such as MCP requests, per-tool calls, per-tool errors, resource reads, card tokens, and explicit note mutations. They never store prompts, queries, note claims, caller identity, or session history.

## Edit-Enabled Workflow

Read tools should still come first. When the server was started with `synrepo mcp --allow-edits`, use:

1. `synrepo_prepare_edit_context` to prepare session-scoped line anchors and compact source context.
2. `synrepo_apply_anchor_edits` to validate prepared anchors, content hashes, and boundary text before writing.

File batches are atomic per file. Multi-file calls can return mixed per-file outcomes and do not claim cross-file transaction semantics. The edit tools do not run shell commands.

## CLI Fallback

If MCP is unavailable, use the CLI rather than blocking:

```bash
synrepo status
synrepo status --recent
synrepo search "term"
synrepo node <target>
synrepo graph query "inbound <target>"
synrepo graph query "outbound <target>"
synrepo graph stats
synrepo reconcile
synrepo findings
```

If neither MCP nor the CLI is available, fall back to normal file reading.
