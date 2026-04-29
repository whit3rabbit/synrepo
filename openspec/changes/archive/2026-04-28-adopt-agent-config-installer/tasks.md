## 1. Dependency add (operator approval gate)

- [x] 1.1 Confirm with the user that adding `agent-config = "0.1"` is approved per the autonomy rules in CLAUDE.md.
- [x] 1.2 Add `agent-config = "0.1"` under `[dependencies]` in `Cargo.toml`. Run `cargo build` and inspect the lockfile diff for transitive deps (`json5`, `jsonc-parser`, `yaml_serde`, `fluent-uri`, `sha2`).
- [x] 1.3 Run `cargo deny check` (if configured) or manually scan licenses for the new transitives. Record results in the PR description.

## 2. Map synrepo's `AgentTool` enum to agent-config integration IDs

- [x] 2.1 In `src/bin/cli_support/agent_shims/mod.rs`, add `AgentTool::agent_config_id(self) -> Option<&'static str>` that returns the matching `agent-config` integration ID (`"claude"`, `"codex"`, `"open-code"` → `"opencode"`, etc.) or `None` for synrepo-only targets (today: `Generic`).
- [x] 2.2 Add `AgentTool::installer_supports_mcp(self) -> bool` returning `agent_config::mcp_by_id(self.agent_config_id()?).is_some()`.
- [x] 2.3 Add `AgentTool::supported_scopes(self) -> &'static [agent_config::ScopeKind]` that delegates through the registered integration.
- [x] 2.4 Add a unit test that asserts every `AgentTool` variant either resolves to a registered installer ID or is on a documented synrepo-only allow-list.

## 3. Rewrite MCP registration to delegate to agent-config

- [x] 3.1 In `src/bin/cli_support/commands/setup/mcp_register.rs`, replace per-tool `setup_*_mcp` functions with one `register_synrepo_mcp(repo_root: &Path, target: AgentTool, scope: agent_config::Scope) -> anyhow::Result<StepOutcome>` that builds an `McpSpec` (`command = "synrepo"`, `args = ["mcp"]` for global, `args = ["mcp", "--repo", "."]` for local), looks up the integration, and calls `install_mcp`.
- [x] 3.2 Map `agent_config::InstallReport` (`created`, `patched`, `already_installed`) to `StepOutcome::Applied | Updated | AlreadyCurrent`.
- [x] 3.3 Surface `AgentConfigError::InlineSecretInLocalScope` as a setup error naming `id` and `key`.
- [x] 3.4 Delete the now-unused per-tool functions (`setup_claude_mcp`, `setup_codex_mcp`, `setup_opencode_mcp`, `setup_cursor_mcp`, `setup_windsurf_mcp`, `setup_roo_mcp`) plus the helper TOML/JSON entry constructors. Confirm `mcp_register.rs` is well under the 400-line cap.
- [x] 3.5 Update `step_register_mcp` in `setup/steps.rs` to call `register_synrepo_mcp` once and stop matching on `AgentTool` variants for dispatch.

## 4. Flip the default scope to global; expose `--project`

- [x] 4.1 In `src/bin/cli_support/cli_args/subcommands.rs`, change the `setup` subcommand's `--global` flag to a `--project` flag with default-off (i.e., default is global). Keep `--global` as a hidden alias that warns it is the new default.
- [x] 4.2 In `setup/steps.rs::ensure_global_supported`, replace the hand-coded match with a call to `AgentTool::supported_scopes()` and choose `Scope::Global` when supported, else fall back to `Scope::Local(repo_root)` with an explicit notice.
- [x] 4.3 Update `setup/orchestration.rs` to thread the resolved `Scope` through the multi-tool dispatcher.
- [x] 4.4 Update the setup output to print the resolved scope on the first line (`Setting up synrepo for {tool} (global)` / `(project)`).

## 5. Rewrite shim/skill/instruction placement to delegate to agent-config

- [x] 5.1 In `src/bin/cli_support/agent_shims/mod.rs`, expose `AgentTool::placement_kind(self) -> ShimPlacement` returning `Skill { name: "synrepo" }`, `Instruction { name: "synrepo", placement: agent_config::InstructionPlacement }`, or `Local` for the `Generic` target.
- [x] 5.2 Replace `agent_setup`'s file-write logic in `cli_support/commands/basic.rs` with a builder that constructs the right spec (`SkillSpec` or `InstructionSpec`), supplies `shim_content(target)` as the body, and calls `install_skill` / `install_instruction` with `owner = "synrepo"`.
- [x] 5.3 Keep the byte-identical drift tests but compare against the spec body rather than against on-disk content alone (so the tests fail before any installer call).
- [x] 5.4 Collapse `output_path` to derive from the agent-config integration's report; keep it as a thin pass-through used by `runtime_probe::shim_output_path` and `report::KNOWN_SHIM_PATHS` so the three sites still reference one source of truth. Update the `every_shim_embeds_doctrine_block` test path if needed.
- [x] 5.5 Verify the `Generic` target (writes `synrepo-agents.md`) still works via a synrepo-local writer; document that path in `agent_shims/mod.rs` as a one-target exception.

## 6. Add the `synrepo remove` ownership-tag wiring

- [x] 6.1 Update `cli_support/commands/setup_mcp_backup.rs` (and any remove paths it backs) to call `uninstall_mcp(<id>, <scope>, "synrepo", "synrepo")` and the analogous `uninstall_skill` / `uninstall_instruction`.
- [x] 6.2 Add a fallback path-based remove that fires when the installer reports "no install owned by synrepo at this path" but a legacy file is present; emit a deprecation warning pointing at `synrepo upgrade --apply`.
- [x] 6.3 Update `agent_shims/registry.rs::record_install_best_effort` to record the resolved scope (global vs project) and the installer-reported file path, not the synrepo-side hard-coded path.

## 7. Migrate pre-existing installs via `synrepo upgrade --apply`

- [x] 7.1 In the `upgrade` command (`cli_support/commands/upgrade.rs` or equivalent), add a "legacy install" detection step that scans the harness configs synrepo would write for unowned `synrepo` entries / shim files.
- [x] 7.2 In `--apply` mode, call `install_mcp` / `install_skill` / `install_instruction` on each detected legacy entry with the current spec content; refuse without explicit confirmation if the on-disk content differs from what we'd write.
- [x] 7.3 Add a "legacy install" drift surface to `synrepo check` so operators can see them before running upgrade.
- [x] 7.4 Update `repair/types/stable.rs` if a new `RepairSurface` or `RepairAction` variant is required (and follow the dual-string-mapping rule documented in `AGENTS.md`).

## 8. Tests

- [x] 8.1 Update `src/bin/cli_support/tests/setup/{codex,misc,...}.rs` to use a tempdir-rooted `Scope::Local` and assert against `InstallReport` plus on-disk content, not against synrepo-side internal paths.
- [x] 8.2 Add `tests/setup/global.rs` covering the new default-global flow for Claude, Codex, OpenCode, Cursor, Windsurf, Roo with `Scope::Global` redirected to a tempdir via `dirs`-fixture.
- [x] 8.3 Add a round-trip test per surface (MCP, skill, instruction): install + uninstall leaves the harness config back to its starting state.
- [x] 8.4 Add an upgrade-migration test: pre-stage a legacy `.mcp.json` synrepo entry with no `_agent_config_tag`, run `upgrade --apply`, assert the entry now carries the marker and ledger record.
- [x] 8.5 Add a default-flip CLI test: `synrepo setup claude` (no flags) writes the global path; `synrepo setup claude --project` writes the project path.
- [x] 8.6 Run `make ci-check` (fmt-check, clippy, serial test).

## 9. Docs and release notes

- [x] 9.1 Update `docs/MCP.md`'s setup section to describe the new global default and the `--project` opt-out.
- [x] 9.2 Update `README.md`'s onboarding block to reflect the global-default behavior.
- [x] 9.3 Add a `CHANGELOG.md` entry under the next release flagging the default-flip and the new dep on `agent-config`.
- [x] 9.4 Update `AGENTS.md` (the "Agent shims and MCP" gotchas) to reference the agent-config installer and remove the duplicated-path-table call-out once the collapse lands.

## 10. Validation

- [x] 10.1 Run `openspec status --change adopt-agent-config-installer --json` and confirm `isComplete=true`.
- [x] 10.2 Run `make ci-check` end-to-end.
- [x] 10.3 Smoke-test `synrepo setup claude` against a real Claude config in a tempdir; verify the `.bak` sibling appears on first patch and the install is idempotent on re-run.
- [x] 10.4 Smoke-test `synrepo upgrade --apply` against a tempdir staged with a legacy synrepo install.
- [x] 10.5 Confirm `make soak-test` still passes on Unix (release-gate suite is unaffected by setup-path changes; this is a regression check).
