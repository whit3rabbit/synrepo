## Why

synrepo Phase 1 is functional — the structural graph is built, rename detection is wired,
and the reconcile loop keeps the graph current. But there is no way for an agent or operator
to see whether the system is working, and no tooling to connect synrepo to other agent CLIs
(Claude Code, Cursor, GitHub Copilot, generic AGENTS.md-based tools).

Three gaps this change closes:

1. **No operational visibility.** After `synrepo init` there is no command that shows the
   current health of the runtime without triggering a full re-bootstrap. Operators cannot
   tell at a glance whether the last reconcile succeeded, the writer lock is free, or the
   graph is current.

2. **skill/SKILL.md describes Phase 2 tools that don't exist yet.** Agents that load the
   skill see MCP tool names and descriptions for a server that isn't running. There is no
   section that explains the current Phase 1 CLI interface to use while waiting for Phase 2.

3. **No integration shims for other agent CLIs.** Claude Code, Cursor, GitHub Copilot, and
   generic AGENTS.md-based tools each have their own conventions for embedding tool guidance.
   There is no way to generate thin integration files that teach these tools to use synrepo's
   CLI commands.

The `synrepo status` command partially addresses gap 1 and is already shipped. This change
addresses gap 2 (skill update) and gap 3 (agent-setup command).

## What Changes

- Update `skill/SKILL.md` to add a "Current phase" section above all MCP tool descriptions.
  This section explains that the MCP server is not yet running, lists the available CLI
  commands for Phase 1, and directs agents to the CLI fallback section.

- Add `synrepo agent-setup <tool>` command that generates a thin integration file for the
  specified agent CLI. The command never modifies existing user configuration; it emits a
  named fragment and prints the instruction for including it.

  Supported targets:
  - `claude` — writes `.claude/synrepo-context.md`; prints `@.claude/synrepo-context.md`
  - `cursor` — writes `.cursor/synrepo.mdc` as a Cursor rule fragment
  - `copilot` — writes `synrepo-copilot-instructions.md`; tells user to paste into
    `.github/copilot-instructions.md`
  - `generic` — writes `synrepo-agents.md`; tells user to paste into `AGENTS.md`

  Each shim is a thin wrapper: it checks for `.synrepo/`, documents the Phase 1 CLI
  commands, describes what synrepo is and is not, and notes when Phase 2 MCP tools land.

## Capabilities

### New Capabilities
- `synrepo agent-setup <tool>` — explicit, on-demand agent integration shim generator

### Modified Capabilities
- `skill/SKILL.md` — add current-phase section; expand CLI fallback command list

## Impact

- Affects `src/bin/cli.rs` and `src/bin/cli_support/commands.rs`
- Adds a new `src/bin/cli_support/agent_setup.rs` or inline shim content module
- Updates `skill/SKILL.md`
- Does not introduce cards, MCP tools, overlay generation, or LLM calls
- Does not modify any existing user configuration files
