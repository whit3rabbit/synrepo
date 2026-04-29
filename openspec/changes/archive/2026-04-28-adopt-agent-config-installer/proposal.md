## Why

`synrepo` is now installed as a global binary, so the synrepo MCP server should default to global registration in agents that support it. The current setup flow hand-rolls per-tool config writers in `src/bin/cli_support/commands/setup/mcp_register.rs` (one function per harness, ~390 lines), supports global only for Claude/Cursor/Windsurf/Roo, gives no `.bak` safety nets, and ships no ownership ledger. The newly-published `agent-config 0.1.0` crate already encodes per-harness file locations, atomic writes with `.bak` backups, idempotent installs, ownership ledgers, and global+local scopes for a wider roster than synrepo currently automates. Adopting it removes maintenance burden, expands global-mode coverage, and aligns with the user's directive that the binary now lives globally.

## What Changes

- Add `agent-config = "0.1"` (`crates.io`, MIT) as a runtime dependency of the `synrepo` binary crate.
- Replace the hand-rolled MCP writers in `src/bin/cli_support/commands/setup/mcp_register.rs` with `agent_config::McpSpec` + `mcp_by_id(...).install_mcp(scope, &spec)` calls. One spec factory, one dispatch per tool.
- **Default `synrepo setup` to `Scope::Global`** when the target harness supports it; expose `--project` (or keep `--global` and flip its default) so callers can opt back into project-scoped writes. Single source of truth for the scope decision lives next to `AgentTool`.
- Use `agent_config::SkillSpec` (where the harness supports skills) and `InstructionSpec` (for instruction-only harnesses) to write the `synrepo` shim files instead of the bespoke output-path table in `src/bin/cli_support/agent_shims/mod.rs::output_path`. The doctrine block content stays authoritative in synrepo and is supplied as the spec body — agent-config only picks the file location and placement mode.
- Adopt the `owner = "synrepo"` ownership tag for every install. `synrepo remove` switches to `uninstall_mcp`/`uninstall_skill`/`uninstall_instruction` keyed on `(name, "synrepo")`, gaining safe coexistence with other consumers via the agent-config ledger.
- Expand the `Automated` `AutomationTier` to cover every harness for which `agent_config::mcp_by_id(id).is_some()` reports MCP support, including ones that are currently `ShimOnly` (Gemini, Cline, plus the second-wave roster covered by agent-config). The synrepo-side roster derived from `AgentTool` stays the public surface; tier promotion is a one-line table change, not a wholesale enum refactor.
- Surface agent-config write paths in setup output and `synrepo status` so operators can see exactly which file changed (matches existing `Registered <scope> MCP server in <path>` lines).

Out of scope: changes to the doctrine text itself, the shim registry naming, the `synrepo agent-setup` CLI shape, or the runtime probe. Those surfaces continue to exist; this change rewires only the *write* path.

## Capabilities

### New Capabilities
- (none — the change rewires existing surfaces.)

### Modified Capabilities
- `bootstrap`: `synrepo setup` defaults to global MCP/skill installation when the harness supports `Scope::Global`; project-scoped installs become the explicit opt-in. Setup uses the agent-config installer for atomic writes, `.bak` backups, and idempotent re-runs.
- `mcp-surface`: MCP server registration into agent configs (Claude `.mcp.json`, Codex `.codex/config.toml`, OpenCode `opencode.json`, Cursor/Windsurf/Roo `.<tool>/mcp.json`, plus newly-supported global paths) is delegated to `agent-config`. The `synrepo mcp` server runtime is unchanged; only the install plumbing moves.
- `agent-doctrine`: shim and skill file placement is delegated to `agent-config` `SkillSpec` / `InstructionSpec`. The doctrine content remains a synrepo-owned compile-time constant; agent-config supplies it as the spec `body` and never authors text.

## Impact

- **Code:** `src/bin/cli_support/commands/setup/mcp_register.rs` shrinks substantially (target: <120 lines, well under the 400-line cap); `src/bin/cli_support/agent_shims/mod.rs::output_path` becomes a thin lookup keyed by agent-config IDs; `src/bin/cli_support/commands/setup/steps.rs::ensure_global_supported` collapses into a `agent_config::by_id(id).supported_scopes()` check.
- **Dependencies:** adds `agent-config 0.1` (transitively pulls `json5`, `jsonc-parser`, `yaml_serde`, `fluent-uri`, `sha2`; `serde_json`, `serde`, `toml_edit`, `thiserror`, `anyhow`, `tempfile`, `dirs` are already in the tree). New runtime dep, MIT, crates.io-published 2026-04-28. Requires user approval per the autonomy rules in CLAUDE.md before landing.
- **CLI flags:** the meaning of `synrepo setup --global` may flip (becomes default-on); preserves a `--project` opt-out. Update `cli_args/subcommands.rs`, the setup wizard, the multi-tool orchestrator, and the docs in `docs/MCP.md`. Behavior change for callers relying on the project-scoped default.
- **Tests:** the per-tool MCP-registration tests in `src/bin/cli_support/tests/setup/` switch from asserting on hand-rolled JSON/TOML shapes to asserting on `agent-config` install reports plus the on-disk byte content the crate produces. Existing assertions about MCP-server entries (e.g., `mcpServers.synrepo.command == "synrepo"`) remain valid because the spec values are the same.
- **Docs:** `docs/MCP.md`, `AGENTS.md` (project boundary entries), and the `Setup` block in `README.md` need a one-paragraph update to describe the new global default and the `--project` opt-out.
- **Removal path:** `synrepo remove` gains owner-tag scoping; existing pre-adoption installs (without the `_agent_config_tag` marker) need a migration shim or a one-off `synrepo upgrade --apply` step. This is the riskiest backward-compat surface and is detailed in `design.md`.
