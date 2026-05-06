## Why

Agents already see synrepo-first guidance in MCP descriptions and skills, but direct grep/read/review workflows can bypass MCP before that guidance has any effect. Client-side lifecycle hooks give Codex and Claude a low-friction reminder to use synrepo cards and compact search for codebase questions, file reviews, and pre-edit context.

## What Changes

- Add nudge-only agent hook support for Codex and Claude.
- Add a hidden `synrepo agent-hook nudge` CLI entrypoint that reads hook JSON from stdin and emits client-valid nudge output or no output.
- Extend setup so Codex and Claude can install local hook configs explicitly.
- Update agent doctrine and docs to cover codebase Q&A, file reviews, broad search, and pre-edit context.
- Keep MCP read-first and avoid any shell proxy, arbitrary command runner, blocking hook, telemetry, background reconcile, or session memory.

## Capabilities

### New Capabilities
- `agent-nudge-hooks`: Client-side nudge hooks that steer supported agents toward synrepo before cold codebase reads, broad searches, reviews, and pre-edit work.

### Modified Capabilities
- `agent-doctrine`: Expands the expected synrepo-first workflow from edit routing to codebase questions, file reviews, search, and review preparation.

## Impact

- Affected runtime paths: CLI argument parsing, CLI dispatch, setup flow, agent integration wizard, local Codex/Claude hook config installers.
- Affected docs and generated guidance: canonical agent doctrine, `skill/SKILL.md`, generated shims, and `docs/MCP.md`.
- No storage schema migration, no MCP server hook interception, no new dependencies, and no automatic background behavior.
