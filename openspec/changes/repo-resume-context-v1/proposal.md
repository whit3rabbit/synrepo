## Why

Agents need a compact way to resume repo work after context loss without asking the user to repeat state or dumping broad repository/session history into the context window. Synrepo already stores the needed repo-scoped facts and advisory surfaces, so this change packages them into an explicit, bounded resume packet.

## What Changes

- Add a repo-scoped resume context packet derived from changed files, handoffs, recent activity, explicit overlay notes, and context metrics.
- Expose the packet through a new MCP tool and CLI command.
- Keep the packet pointer-first: full details remain behind existing bounded tools and commands.
- Preserve the product boundary: no automatic hook capture, no prompt logs, no raw tool-output history, and no generic session memory.

## Capabilities

### New Capabilities
- `repo-resume-context`: Defines the explicit repo resume packet contract, sections, limits, budget trimming, and source-truth boundaries.

### Modified Capabilities
- `mcp-surface`: Adds `synrepo_resume_context` as a repo-addressable MCP tool.
- `context-accounting`: Counts resume-context responses with aggregate metrics only.
- `agent-doctrine`: Tells agents to call the resume packet after stale resumes before asking the user to repeat repo context.

## Impact

- Affected code: surface packet collector, CLI args/dispatch, MCP tool registration, docs, agent doctrine/shims, and tests.
- No new dependencies.
- No graph schema, overlay schema, source-edit, or hook behavior changes.
