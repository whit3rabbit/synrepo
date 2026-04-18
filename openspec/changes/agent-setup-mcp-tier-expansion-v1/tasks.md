## 1. Verify target config formats

- [ ] 1.1 Confirm Cursor writes MCP config at `.cursor/mcp.json` with `mcpServers.{name}.{command, args}` shape — check Cursor docs or a real `.cursor/mcp.json` in a repo that integrates with Cursor.
- [ ] 1.2 Confirm Windsurf supports project-scoped `.windsurf/mcp.json` with the same shape.
- [ ] 1.3 Confirm Roo Code reads `.roo/mcp.json` with the same shape.
- [ ] 1.4 If any of the three diverges (e.g., different key name, required extra field), record the difference and adapt the writer below.

## 2. Move tier assignment

- [ ] 2.1 In `src/bin/cli_support/agent_shims/mod.rs:98–114`, move `AgentTool::Cursor`, `AgentTool::Windsurf`, `AgentTool::Roo` from the `ShimOnly` match arm to the `Automated` arm.
- [ ] 2.2 Build with `cargo check --bins` — expect the `automation_tier_matches_step_register_mcp_dispatch` test to fail until the dispatch in step 3 is added.

## 3. Implement MCP writers

- [ ] 3.1 In `src/bin/cli_support/commands/setup.rs`, add `setup_cursor_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome>`. Copy the shape of `setup_claude_mcp`; change the path to `.cursor/mcp.json`.
- [ ] 3.2 Add `setup_windsurf_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome>`. Path: `.windsurf/mcp.json`.
- [ ] 3.3 Add `setup_roo_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome>`. Path: `.roo/mcp.json`.
- [ ] 3.4 Extend `step_register_mcp` dispatch at `setup.rs:90–108` with three new `match` arms: `AgentTool::Cursor => setup_cursor_mcp(repo_root)`, and similar for Windsurf and Roo.

## 4. Update include_instruction

- [ ] 4.1 In `agent_shims/mod.rs:175–218`, update the three `include_instruction` arms:
  - Cursor: `"The MCP server is registered in .cursor/mcp.json. The rule fragment is in .cursor/synrepo.mdc — enable it in your Cursor rules."`
  - Windsurf: `"Windsurf loads .windsurf/rules/synrepo.md as a project rule automatically. The MCP server is registered in .windsurf/mcp.json."`
  - Roo: `"Roo Code loads .roo/commands/synrepo.md automatically. The MCP server is registered in .roo/mcp.json."`

## 5. Update shim content

- [ ] 5.1 In `src/bin/cli_support/agent_shims/shims.rs`, locate the MCP-registration paragraph inside `CURSOR_SHIM`, `WINDSURF_SHIM`, and `ROO_SHIM`.
- [ ] 5.2 Replace the "register synrepo mcp --repo . manually" paragraph with an "automated" paragraph matching the first-tier shims' framing.
- [ ] 5.3 Confirm the `every_shim_embeds_doctrine_block` test still passes (the doctrine block is unchanged).

## 6. Tests

- [ ] 6.1 In `src/bin/cli_support/commands/setup.rs` tests module, add `setup_cursor_mcp_creates_config_when_missing` following the pattern of `setup_claude_mcp_creates_config_when_missing`.
- [ ] 6.2 Add `setup_cursor_mcp_idempotent_on_rerun`.
- [ ] 6.3 Add `setup_cursor_mcp_merges_into_existing_entries`.
- [ ] 6.4 Add `setup_cursor_mcp_refuses_invalid_json`.
- [ ] 6.5 Repeat 6.1–6.4 for `setup_windsurf_mcp`.
- [ ] 6.6 Repeat 6.1–6.4 for `setup_roo_mcp`.
- [ ] 6.7 Re-run `cargo test -p synrepo automation_tier_matches_step_register_mcp_dispatch` — should pass once 2.1 and 3.4 are both done.

## 7. Docs

- [ ] 7.1 Update `AGENTS.md` "Shipped CLI surface (export-and-polish-v1)" — the `synrepo agent-setup` bullet lists Automated vs ShimOnly agents. Move `cursor`, `windsurf`, `roo` into the Automated list.
- [ ] 7.2 Update the CLAUDE.md/AGENTS.md "Commands" list to reflect the broader Automated tier in the `agent-setup` description.

## 8. Verification

- [ ] 8.1 `make check` passes (fmt, clippy, parallel tests).
- [ ] 8.2 Smoke test on a scratch repo:
  - `synrepo agent-setup cursor` → verify `.cursor/synrepo.mdc` and `.cursor/mcp.json` both land; the latter contains the synrepo server entry.
  - `synrepo agent-setup windsurf` → verify `.windsurf/rules/synrepo.md` and `.windsurf/mcp.json`.
  - `synrepo agent-setup roo` → verify `.roo/commands/synrepo.md` and `.roo/mcp.json`.
- [ ] 8.3 Re-run each on an already-set-up repo; confirm the writer reports "already current" rather than rewriting.

## 9. Archive

- [ ] 9.1 Run `openspec validate agent-setup-mcp-tier-expansion-v1 --strict`.
- [ ] 9.2 Invoke `opsx:archive` with change id `agent-setup-mcp-tier-expansion-v1`.
