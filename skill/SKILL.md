---
name: synrepo
description: Use synrepo when working in a repository that contains a .synrepo/ directory. synrepo precomputes structural facts about the codebase and serves them as small token-budgeted cards through an MCP server. Reach for synrepo tools BEFORE reading source files cold.
---

# synrepo — skill for Claude Code

synrepo is a context compiler. It turns a repository into small, deterministic, task-shaped packets called **cards** that Claude can query through an MCP server instead of reading whole files.

## Cleanest Workflow (Binary First)

The recommended setup for a new repository is:

1.  **Install local binary**: Ensure `synrepo` is available in your PATH.
2.  **Initialize and Config**: Run `synrepo setup claude` in the repository root. This runs `synrepo init`, writes instructions, and registers the project-scoped MCP server in `.mcp.json`.
3.  **Handoff to Claude Code**: Claude Code will automatically detect the MCP server and instructions.
4.  **Watch Mode (Optional)**: Run `synrepo watch --daemon` if you want background refresh as you edit.

## Current surface

After setup, the MCP server exposes **twenty-one tools** for repository traversal and inspection. Use `synrepo_search` to find node IDs (format: `file_0000000000000042`, `symbol_0000000000000024`) before calling card or investigation tools.

### Primary Tools
| Tool | What it does |
| --- | --- |
| `synrepo_overview` | Graph stats + mode; your first call per session |
| `synrepo_card(target, budget?)` | SymbolCard or FileCard for orientation and navigation |
| `synrepo_search(query)` | Lexical search across indexed files |
| `synrepo_where_to_edit(task)` | Search-ranked file suggestions for a task description |
| `synrepo_change_impact(target)` | Approximate inbound dependencies showing what depends on target |
| `synrepo_change_risk(target)` | Composite risk signal (drift + co-change + hotspots) |

## Agent doctrine

synrepo is a code-context compiler. When `.synrepo/` exists in the repo root, prefer MCP tools (or the CLI fallback) over cold file reads for orientation and navigation.

### Default path

1. Start with search or entry-point discovery to find candidates.
2. Use `tiny` cards to orient and route.
3. Use `normal` cards to understand a neighborhood.
4. Use `deep` cards only before writing code, or when exact source or body details matter.

Overlay commentary and proposed cross-links are advisory, labeled machine-authored, and freshness-sensitive. Treat stale labels as information, not as errors. **Refresh is explicit**: every tool returns what is currently in the overlay. To get fresh commentary after a code change, you must call `synrepo_refresh_commentary(target)`.

### Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.
- Do not trigger synthesis (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.

### Product boundary

- synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
- Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.

## The core mental model

There are two kinds of content in synrepo, and the distinction matters:

- **Graph content** — facts that tree-sitter, git, or humans declared directly. Tagged `source_store: graph`. **Treat graph content as the primary source of truth.**
- **Overlay content** — things the LLM proposed (advisory commentary, findings). Tagged `source_store: overlay`. **Treat overlay content as helpful context, not ground truth.**

## CLI fallback

If the MCP server is not running, use the CLI directly:
- `synrepo status` — health and graph counts
- `synrepo search <query>` — find symbols or files
- `synrepo node <id>` — dump node metadata
- `synrepo graph query "inbound <id>"` — reverse dependencies
- `synrepo reconcile` — refresh the graph after source changes
