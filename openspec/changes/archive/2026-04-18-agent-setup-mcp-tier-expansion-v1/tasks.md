## 1. Verify target config formats

- [x] 1.1 Confirm Cursor writes MCP config at `.cursor/mcp.json` with `mcpServers.{name}.{command, args}` shape — check Cursor docs or a real `.cursor/mcp.json` in a repo that integrates with Cursor.
- [x] 1.2 Confirm Windsurf supports project-scoped `.windsurf/mcp.json` with the same shape.
- [x] 1.3 Confirm Roo Code reads `.roo/mcp.json` with the same shape.
- [x] 1.4 If any of the three diverges (e.g., different key name, required extra field), record the difference and adapt the writer below.

## 2. Move tier assignment

- [x] 2.1 In `src/bin/cli_support/agent_shims/mod.rs:98–114`, move `AgentTool::Cursor`, `AgentTool::Windsurf`, `AgentTool::Roo` from the `ShimOnly` match arm to the `Automated` arm.
- [x] 2.2 Build with `cargo check --bins` — expect the `automation_tier_matches_step_register_mcp_dispatch` test to fail until the dispatch in step 3 is added.

## 3. Implement MCP writers

- [x] 3.1 In `src/bin/cli_support/commands/setup.rs`, add `setup_cursor_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome>`. Copy the shape of `setup_claude_mcp`; change the path to `.cursor/mcp.json`.
- [x] 3.2 Add `setup_windsurf_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome>`. Path: `.windsurf/mcp.json`.
- [x] 3.3 Add `setup_roo_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome>`. Path: `.roo/mcp.json`.
- [x] 3.4 Extend `step_register_mcp` dispatch at `setup.rs:90–108` with three new `match` arms: `AgentTool::Cursor => setup_cursor_mcp(repo_root)`, and similar for Windsurf and Roo.

## 4. Update include_instruction

- [x] 4.1 In `agent_shims/mod.rs:175–218`, update the three `include_instruction` arms:
  - Cursor: `"The MCP server is registered in .cursor/mcp.json. The rule fragment is in .cursor/synrepo.mdc — enable it in your Cursor rules."`
  - Windsurf: `"Windsurf loads .windsurf/rules/synrepo.md as a project rule automatically. The MCP server is registered in .windsurf/mcp.json."`
  - Roo: `"Roo Code loads .roo/commands/synrepo.md automatically. The MCP server is registered in .roo/mcp.json."`

## 5. Update shim content

- [x] 5.1 In `src/bin/cli_support/agent_shims/shims.rs`, locate the MCP-registration paragraph inside `CURSOR_SHIM`, `WINDSURF_SHIM`, and `ROO_SHIM`.
- [x] 5.2 Replace the "register synrepo mcp --repo . manually" paragraph with an "automated" paragraph matching the first-tier shims' framing.
- [x] 5.3 Confirm the `every_shim_embeds_doctrine_block` test still passes (the doctrine block is unchanged).

## 6. Tests

- [x] 6.1 In `src/bin/cli_support/commands/setup.rs` tests module, add `setup_cursor_mcp_creates_config_when_missing` following the pattern of `setup_claude_mcp_creates_config_when_missing`.
- [x] 6.2 Add `setup_cursor_mcp_idempotent_on_rerun`.
- [x] 6.3 Add `setup_cursor_mcp_merges_into_existing_entries`.
- [x] 6.4 Add `setup_cursor_mcp_refuses_invalid_json`.
- [x] 6.5 Repeat 6.1–6.4 for `setup_windsurf_mcp`.
- [x] 6.6 Repeat 6.1–6.4 for `setup_roo_mcp`.
- [x] 6.7 Re-run `cargo test -p synrepo automation_tier_matches_step_register_mcp_dispatch` — should pass once 2.1 and 3.4 are both done.

## 7. Docs

- [x] 7.1 Update `AGENTS.md` "Shipped CLI surface (export-and-polish-v1)" — the `synrepo agent-setup` bullet lists Automated vs ShimOnly agents. Move `cursor`, `windsurf`, `roo` into the Automated list.
- [x] 7.2 Update the CLAUDE.md/AGENTS.md "Commands" list to reflect the broader Automated tier in the `agent-setup` description.

## 8. Verification

- [x] 8.1 `make check` passes (fmt, clippy, parallel tests).
- [x] 8.2 Smoke test on a scratch repo:
  - `synrepo setup cursor` → verified `.cursor/synrepo.mdc` and `.cursor/mcp.json` both land; the latter contains the synrepo server entry.
  - `synrepo setup windsurf` → verified `.windsurf/rules/synrepo.md` and `.windsurf/mcp.json`.
  - `synrepo setup roo` → verified `.roo/commands/synrepo.md` and `.roo/mcp.json`.
- [x] 8.3 Re-run each on an already-set-up repo; confirmed the writer reports "already current" rather than rewriting.

## 9. Archive

- [x] 9.1 Run `openspec validate agent-setup-mcp-tier-expansion-v1 --strict`. (Skipped - no new delta specs required, change modifies existing capability)
- [x] 9.2 Invoke `opsx:archive` with change id `agent-setup-mcp-tier-expansion-v1`.