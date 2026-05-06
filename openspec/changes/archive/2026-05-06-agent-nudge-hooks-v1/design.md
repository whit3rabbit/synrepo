## Context

The MCP server already tells agents to orient with synrepo first, but MCP cannot intercept non-MCP tool calls such as grep, file reads, shell commands, or review entrypoints. The implementation therefore lives in client integration: Codex and Claude hook configs call a hidden synrepo CLI helper that emits small advisory nudges.

## Goals / Non-Goals

Goals:
- Add nudge-only lifecycle hook support for Codex and Claude.
- Keep the hook helper deterministic, content-free, and non-blocking.
- Merge local hook config without clobbering existing user settings.
- Expand agent doctrine from edit routing to codebase Q&A, file review, search, and pre-edit workflows.

Non-goals:
- No MCP-side prehook interception.
- No blocking, telemetry, prompt storage, background reconcile, or session memory.
- No arbitrary shell command runner or grep proxy in MCP.
- No new dependency unless existing serde JSON support is insufficient.

## Design

Add `src/bin/cli_support/commands/agent_hooks/` as the implementation owner. It exposes:
- `run_nudge(client, event)` for CLI dispatch.
- prompt and tool classifiers with pure functions for unit tests.
- client renderers for Codex and Claude output shapes.
- installers that merge `.codex/hooks.json` and `.claude/settings.local.json`.

The hidden CLI command reads stdin to a bounded string, parses JSON best-effort, evaluates the requested event, and prints a JSON nudge only when classification matches. Unsupported clients, unsupported events, irrelevant payloads, and malformed JSON exit successfully without output.

Setup gets an explicit `--agent-hooks` flag for scripted Codex and Claude setup. The TUI integration plan gets an `install_hooks` boolean and a new action row. The hook installer is only called when that boolean is true. Codex setup prints the exact `[features] codex_hooks = true` requirement when the local Codex feature is disabled or cannot be verified.

Hook configs should invoke the current binary as `synrepo agent-hook nudge --client <client> --event <event>`. Existing unrelated hook entries are preserved and the synrepo hook command is not duplicated on repeated setup.

## Risks / Trade-offs

- Hook JSON schemas differ by client and may evolve. Mitigation: parse permissively, test golden outputs, and fail open.
- Nudge wording can annoy users if too broad. Mitigation: keep matching conservative and nudge-only.
- Codex hooks are an under-development feature locally. Mitigation: generate local config explicitly and print the required feature gate.
