## 1. Canonical doctrine constant

- [x] 1.1 Draft the final `DOCTRINE_BLOCK` text in `src/bin/cli_support/agent_shims/doctrine.rs` (agent_shims was already at 431 lines; split into submodule directory first). Content covers: synrepo identification, default escalation path, overlay advisory rule, four do-not rules, three product-boundary rules.
- [x] 1.2 Draft the shorter `TOOL_DESC_ESCALATION_LINE` constant (one sentence) for reuse in MCP card-returning tool descriptions. Marked `#[allow(dead_code)]` pending §4 MCP wiring.
- [x] 1.3 Add module-level documentation comment pointing to `docs/FOUNDATION.md` §"Product boundaries and doctrine" as the source that the block tracks.

## 2. Shim rewrites

- [x] 2.1 Rewrite `CLAUDE_SHIM` to embed `doctrine_block!()` via `concat!`, keeping Claude-specific MCP invocation text and removing duplicate escalation copy.
- [x] 2.2 Rewrite `CURSOR_SHIM` the same way; preserve the MDC frontmatter block at the top.
- [x] 2.3 Rewrite `COPILOT_SHIM` the same way.
- [x] 2.4 Rewrite `GENERIC_SHIM` the same way.
- [x] 2.5 Rewrite `CODEX_SHIM` the same way.
- [x] 2.6 Rewrite `WINDSURF_SHIM` the same way.
- [x] 2.7 Verify each shim still contains its target-specific tool-invocation details (how the agent calls MCP tools under that target's conventions).

## 3. SKILL.md alignment

- [ ] 3.1 Rewrite the escalation section of `skill/SKILL.md` so its wording is a superset of `DOCTRINE_BLOCK` (SKILL.md can expand with examples; the core block text must appear verbatim).
- [ ] 3.2 Add a "Do not" subsection in `skill/SKILL.md` using the same four bullets as the block.
- [ ] 3.3 Add a "Product boundary" subsection in `skill/SKILL.md` using the same three bullets as the block.
- [ ] 3.4 Collapse any competing examples so the four canonical examples match the ones in §1 (unfamiliar repo, where to edit, change impact, open source body).

## 4. MCP tool descriptions

- [ ] 4.1 Audit card-returning tool descriptions in `crates/synrepo-mcp/` (`synrepo_card`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_entrypoints`, `synrepo_where_to_edit`, `synrepo_change_impact`).
- [ ] 4.2 Append `TOOL_DESC_ESCALATION_LINE` to each listed description exactly once.
- [ ] 4.3 Confirm non-card tools (`synrepo_search`, `synrepo_findings`, `synrepo_recent_activity`, `synrepo_overview`) do not get the escalation line; their default-budget semantics differ.
- [ ] 4.4 Verify resulting descriptions render as intended in an MCP `list-tools` response (manual smoke via `synrepo-mcp` stdio).

## 5. Bootstrap report update

- [ ] 5.1 In `src/bootstrap/report.rs` success path, add a one-line pointer: `"Agent doctrine: tiny → normal → deep. See <shim-path> for the full protocol."` The shim path is resolved from the target the user most recently ran `agent-setup` against, or falls back to a generic instruction when `agent-setup` has not been run.
- [ ] 5.2 Add a unit test asserting the pointer line is present in the success output.
- [ ] 5.3 Confirm the failure/partial-health path is unchanged (the pointer appears only on clean success).

## 6. Tests

- [x] 6.1 Unit test in `src/bin/cli_support/agent_shims/tests.rs` (`doctrine_block_size_is_bounded`): asserts `DOCTRINE_BLOCK.is_empty() == false` and `DOCTRINE_BLOCK.len() < 4096`.
- [x] 6.2 Unit test `every_shim_embeds_doctrine_block`: for each variant of `AgentTool`, asserts `shim.contains(DOCTRINE_BLOCK)`. Byte-identical guarantee enforced; passes on 6/6 shims. Also added `doctrine_block_covers_required_sections`.
- [ ] 6.3 Integration test: read `skill/SKILL.md`, assert it contains the opening sentence of `DOCTRINE_BLOCK` and each of the four do-not bullets and each of the three product-boundary bullets verbatim.
- [ ] 6.4 Unit test for MCP tool descriptions: for each card-returning tool listed in task 4.1, assert its description contains `TOOL_DESC_ESCALATION_LINE`.
- [ ] 6.5 Snapshot test (insta): snapshot the full text of each shim so future diff review is explicit when the block or target-specific text changes.

## 7. Validation

- [ ] 7.1 Run `cargo test` and confirm all tests pass.
- [ ] 7.2 Run `cargo clippy --workspace --all-targets -- -D warnings` and confirm no new warnings.
- [ ] 7.3 Run `make check` for full CI-equivalent validation.
- [ ] 7.4 Smoke test: `cargo run -- agent-setup claude`, inspect `.claude/synrepo-context.md` contains the doctrine block verbatim. Repeat for `cursor`, `copilot`, `generic`, `codex`, `windsurf`.
- [ ] 7.5 Smoke test: `synrepo-mcp` startup; `list-tools` response includes the escalation line on each card-returning tool exactly once.
- [ ] 7.6 Smoke test: `cargo run -- init` on a fresh clone; success output contains the doctrine pointer line.

## 8. Documentation

- [ ] 8.1 Update `ROADMAP.md` §11 completion checkpoint to note Milestone A complete when this change archives.
- [ ] 8.2 No updates to `docs/FOUNDATION.md` or `docs/FOUNDATION-SPEC.md` (already correct after 2026-04-14).
- [x] 8.3 Added entry under "Gotchas" in `AGENTS.md` pointing at `doctrine.rs` / `shims.rs` split and naming the `every_shim_embeds_doctrine_block` test that enforces byte-identity.
