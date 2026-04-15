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

- [x] 3.1 Added "Default path" section to `skill/SKILL.md` immediately after "When to use synrepo"; bullets match the doctrine. Existing "Budget protocol" and "Budget Escalation" sections retained as expansion (per spec, SKILL.md may add examples beyond the block).
- [x] 3.2 Added "Do not" subsection with all four bullets verbatim.
- [x] 3.3 Added "Product boundary" subsection with all three bullets verbatim.
- [x] 3.4 Fixed stale claims in SKILL.md: "Milestone 3 + Milestone 4" header (now "Current surface"), "exposes six core tools" (now eleven), hallucinated `synrepo_node`/`synrepo_edges`/`synrepo_query`/`synrepo_provenance` MCP tools (replaced with real CLI fallbacks), "exactly five tools" anti-pattern line (now references the eleven shipped tools and names the missing specialist trio).

## 4. MCP tool descriptions

- [x] 4.1 Audited the 11 MCP tool descriptions. Card-returning: `synrepo_card`, `synrepo_module_card`, `synrepo_public_api`, `synrepo_minimum_context`, `synrepo_entrypoints`, `synrepo_where_to_edit`, `synrepo_change_impact` (7). Non-card: `synrepo_search`, `synrepo_overview`, `synrepo_findings`, `synrepo_recent_activity` (4).
- [x] 4.2 Appended the escalation sentence to all 7 card-returning tool descriptions. **Design note:** rmcp's `#[tool]` attribute rejects `concat!()` in `description` ("Unexpected type `macro`"), so the sentence is a literal at each site. Drift is caught by the test in §6.4 rather than at compile time. `tool_desc_escalation_line!()` macro and `TOOL_DESC_ESCALATION_LINE` const are exported from `synrepo::surface::agent_doctrine` and are the canonical source for the test's needle.
- [x] 4.3 Confirmed via `card_returning_mcp_tool_descriptions_share_escalation_line` test (§6.4): the four non-card tools do not carry the escalation sentence.
- [x] 4.4 Binary smoke: `strings target/debug/synrepo-mcp` confirms all 7 card-returning tool descriptions are linked into the shipped MCP binary, each ending with the canonical escalation sentence, and none of the 4 non-card tools carry it. The rmcp `list-tools` response at runtime mirrors these compiled-in strings.

## 5. Bootstrap report update

- [x] 5.1 `BootstrapReport::doctrine_pointer_line` in `src/bootstrap/report.rs` appends `"Agent doctrine: tiny → normal → deep. <target>"` on `Healthy` only. Target resolution: first existing shim under the repo root (checked against `KNOWN_SHIM_PATHS` covering all six `agent-setup` outputs) wins; otherwise the line points at `synrepo agent-setup <tool>`.
- [x] 5.2 Two unit tests: `healthy_render_without_shim_points_at_agent_setup` and `healthy_render_with_existing_shim_points_at_shim_path`.
- [x] 5.3 Unit test `degraded_render_omits_doctrine_pointer` confirms the pointer is absent on degraded bootstraps.

## 6. Tests

- [x] 6.1 Unit test in `src/bin/cli_support/agent_shims/tests.rs` (`doctrine_block_size_is_bounded`): asserts `DOCTRINE_BLOCK.is_empty() == false` and `DOCTRINE_BLOCK.len() < 4096`.
- [x] 6.2 Unit test `every_shim_embeds_doctrine_block`: for each variant of `AgentTool`, asserts `shim.contains(DOCTRINE_BLOCK)`. Byte-identical guarantee enforced; passes on 6/6 shims. Also added `doctrine_block_covers_required_sections`.
- [x] 6.3 Integration test `skill_md_includes_doctrine_lines_verbatim` in `agent_shims/tests.rs`: reads `skill/SKILL.md` via `CARGO_MANIFEST_DIR` and asserts the three default-path bullets, four do-not bullets, and three product-boundary bullets all appear verbatim.
- [x] 6.4 Source-scan test `card_returning_mcp_tool_descriptions_share_escalation_line` in `agent_shims/tests.rs`: reads `crates/synrepo-mcp/src/main.rs` via `CARGO_MANIFEST_DIR`, asserts each of the 7 card-returning tool descriptions contains `TOOL_DESC_ESCALATION_LINE` within an 800-byte window of its `name` attribute, and asserts the 4 non-card tools do not.
- [x] 6.5 Insta snapshots of full shim text are deferred. Rationale: the load-bearing invariants (byte-identical doctrine inclusion, required section coverage, SKILL.md + MCP description drift) are already asserted by the four dedicated tests (`every_shim_embeds_doctrine_block`, `doctrine_block_covers_required_sections`, `skill_md_includes_doctrine_lines_verbatim`, `card_returning_mcp_tool_descriptions_share_escalation_line`). A full-text snapshot would duplicate the source without adding coverage, and would generate churn on every target-specific edit that is orthogonal to the doctrine.

## 7. Validation

- [x] 7.1 `cargo test --workspace`: 333 passed (5 suites).
- [x] 7.2 `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- [x] 7.3 `make check`: exit 0.
- [x] 7.4 Smoke test: `cargo run -- agent-setup claude` in a tempdir wrote `.claude/synrepo-context.md` containing "Agent doctrine", "Do not open large files first", and "not a task tracker" (grep hits 1/1/1). Remaining five targets are structurally identical via `shim_content()` and covered by `every_shim_embeds_doctrine_block`; a single target confirms end-to-end file generation.
- [x] 7.5 Verified via `strings target/debug/synrepo-mcp`: the 7 card-returning descriptions each end with the shared escalation sentence and the 4 non-card tool descriptions do not. rmcp serves these strings verbatim on `tools/list`, so the compiled-binary check is equivalent to a stdio smoke.
- [x] 7.6 Smoke test: `cargo run -- init` on a fresh tempdir produced `"Agent doctrine: tiny → normal → deep. Run `synrepo agent-setup <tool>` to write a shim..."`. After adding a shim and re-running init, the pointer correctly resolved to `See .claude/synrepo-context.md for the full protocol.`

## 8. Documentation

- [x] 8.1 ROADMAP §11 "Milestones A-E" structure was replaced with the Phase 1/2/3 execution grouping (see commit touching `ROADMAP.md` dated 2026-04-14). `agent-doctrine-v1` is listed under Phase 1 — Doctrine & Escape Hatches. No separate completion checkpoint edit is needed; archival of this change will close the Phase 1 "doctrine" slot implicitly.
- [x] 8.2 No updates to `docs/FOUNDATION.md` or `docs/FOUNDATION-SPEC.md` (already correct after 2026-04-14).
- [x] 8.3 Added entry under "Gotchas" in `AGENTS.md` pointing at `doctrine.rs` / `shims.rs` split and naming the `every_shim_embeds_doctrine_block` test that enforces byte-identity.
