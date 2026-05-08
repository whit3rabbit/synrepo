# Setup And Fallback

## Project-scoped and global MCP

Project-scoped MCP configs that launch `synrepo mcp --repo .` have a default repository, so `repo_root` may be omitted. Passing the absolute repository root is still valid and preferred when you can identify it reliably.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools, or call `synrepo_use_project(repo_root)` once to set the session default.

Repo-local setup and removal are backed by `agent-config`: `synrepo setup <tool> --project` installs local MCP, skills, or instructions for every supported registry target, and `synrepo remove [tool] --apply` removes the owned entries through the same ledger. Legacy unowned `synrepo` entries may be removed with a warning.

Resource-aware MCP hosts may also address managed projects explicitly with `synrepo://project/{project_id}/card/{target}`, `synrepo://project/{project_id}/file/{path}/outline`, or `synrepo://project/{project_id}/context-pack?goal={goal}`. Use `synrepo://projects` to list stable project IDs.

If a tool reports that a repository is not managed by synrepo, ask the user to run:

```bash
synrepo project add <path>
```

Do not bypass registry gating.

## CLI fallback

Use `st`, `rg`, direct file reads, or normal repository tools when:

- MCP tools are unavailable
- `synrepo_search` returns zero results for exact tokens that should exist
- the graph is stale or compatibility requires rebuild
- the code path is not represented in graph-backed cards
- raw source ranges are required for patching
- tests, formatting, linting, or build commands must be run

Do not treat CLI fallback as failure. Treat it as raw-source verification after bounded synrepo routing.

If MCP is unavailable, use the CLI:

```bash
synrepo status
synrepo resume-context --json
synrepo check
synrepo task-route "find auth entrypoints"
synrepo search "term"
synrepo graph stats
synrepo reconcile
synrepo sync
```

If neither MCP nor the CLI is available, fall back to normal file reading.
