## Why

`synrepo agent-setup <tool>` has a two-tier support matrix (`src/bin/cli_support/agent_shims/mod.rs:34–40`):

- **Automated**: shim + MCP config written by `synrepo setup`. Today: Claude, Codex, OpenCode.
- **ShimOnly**: shim written; operator wires the MCP server entry by hand into the agent's own config.

Cursor, Windsurf, and Roo Code all have documented project-scoped MCP config formats:

- **Cursor**: `.cursor/mcp.json` (same JSON schema as Claude's `.mcp.json`, documented in Cursor's MCP docs).
- **Windsurf**: `~/.codeium/windsurf/mcp_config.json` (user-level) with project-override precedence for `.windsurf/mcp.json`; Windsurf docs list both paths.
- **Roo Code**: `.roo/mcp.json` (JSON format, project-scoped).

All three are currently in the `ShimOnly` tier at `agent_shims/mod.rs:101–112`. Operators see a message like `"Cursor uses shim-only integration; register synrepo mcp --repo . as a stdio MCP server in the agent's own config"` (lines 98–107 of `src/bin/cli_support/commands/setup.rs`). This is a friction point that `synrepo setup` is explicitly designed to eliminate for the first-tier agents and has no technical reason to skip for these three.

The existing test `automation_tier_matches_step_register_mcp_dispatch` in `agent_shims/tests.rs` enforces that the two-tier matrix and the dispatch in `step_register_mcp` (`setup.rs:90–108`) agree — so adding an agent to `Automated` without implementing the MCP writer makes CI fail loud. This is the right safety net.

## What Changes

- Promote `AgentTool::Cursor`, `AgentTool::Windsurf`, `AgentTool::Roo` from `AutomationTier::ShimOnly` to `AutomationTier::Automated` in `src/bin/cli_support/agent_shims/mod.rs:98–114`.
- Implement three new MCP-registration functions in `src/bin/cli_support/commands/setup.rs`:
  - `setup_cursor_mcp(repo_root) -> StepOutcome` — writes `.cursor/mcp.json` following the same JSON-merge pattern as `setup_claude_mcp`. Entry shape: `{ "command": "synrepo", "args": ["mcp", "--repo", "."] }` under `mcpServers.synrepo`.
  - `setup_windsurf_mcp(repo_root) -> StepOutcome` — writes `.windsurf/mcp.json` with the same shape. Windsurf supports project-scoped overrides; the global config at `~/.codeium/windsurf/mcp_config.json` is deliberately left alone (project override beats user config).
  - `setup_roo_mcp(repo_root) -> StepOutcome` — writes `.roo/mcp.json`. Roo uses the same shape as Claude and Cursor.
- Extend the dispatch in `step_register_mcp` (`setup.rs:90–108`) to include the three new targets.
- Update `include_instruction` for the three promoted targets to tell users the MCP entry was registered automatically (mirroring Claude/Codex/OpenCode phrasing).
- Add three tests (one per new writer) following the pattern of `setup_claude_mcp_*` in `setup.rs` tests:
  - Empty config creation.
  - Idempotent re-run (already-current).
  - Merge into existing user-authored entries.
  - Refuse to overwrite invalid JSON.
- Update the agent shim copy for Cursor, Windsurf, and Roo to reflect that MCP registration is now scripted (remove the "register MCP server manually" paragraph, add the "tools list" framing used by first-tier agents).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-setup`: three additional agents (Cursor, Windsurf, Roo Code) now receive automated MCP registration instead of a "wire it up manually" message.

## Impact

- **Code**:
  - `src/bin/cli_support/agent_shims/mod.rs:98–114` — move Cursor, Windsurf, Roo to the `Automated` arm.
  - `src/bin/cli_support/agent_shims/mod.rs:175–218` — update `include_instruction` for the three agents.
  - `src/bin/cli_support/agent_shims/shims.rs` — rewrite the MCP-registration section of the Cursor, Windsurf, and Roo shim content to describe the automated flow.
  - `src/bin/cli_support/commands/setup.rs` — three new `setup_*_mcp` functions; three new dispatch arms in `step_register_mcp`.
  - `src/bin/cli_support/commands/setup.rs` tests — six-nine new test cases per the pattern above.
  - `src/bin/cli_support/agent_shims/tests.rs` — the existing `automation_tier_matches_step_register_mcp_dispatch` test will auto-extend its coverage when the dispatch gains the new arms.
- **APIs**: None public. `synrepo agent-setup cursor|windsurf|roo` now produces a different message; the command surface is unchanged.
- **Storage**: Writes three new project-scoped config paths. No synrepo storage change.
- **Dependencies**: None.
- **Docs**:
  - `AGENTS.md` "Shipped CLI surface (export-and-polish-v1)" — update the agent-setup bullet's Automated/ShimOnly lists to reflect the three promoted agents.
  - `skill/SKILL.md` — no change; the skill doc describes the MCP contract, not the per-agent registration path.
- **Systems**: No cross-change dependency.

## Notes on prior art

The existing Claude / Codex / OpenCode writers in `setup.rs` use three different config formats (JSON object, TOML, JSON string value) because the agents themselves differ. Cursor, Windsurf, and Roo all use the same JSON-object format as Claude, which means the new writers are close to a direct copy of `setup_claude_mcp` with different file paths. This keeps the implementation small and the test shape identical.
