## Context

`synrepo` is now distributed as a globally-installed binary. The current onboarding flow in `src/bin/cli_support/commands/setup/` was written when the binary was project-local: it hand-rolls per-tool MCP-config writers, supports `--global` only for Claude/Cursor/Windsurf/Roo, and uses a synrepo-side output-path table for shim files. The newly published [`agent-config 0.1.0`](https://crates.io/crates/agent-config) crate (MIT, published 2026-04-28T22:22:28Z) was designed for this exact problem: a thin generic installer that knows where each harness keeps hooks/MCP servers/skills/instructions, performs atomic writes with `.bak` first-touch backups, supports `Scope::Global` and `Scope::Local(<path>)`, and tags every install with an owner string for safe coexistence and clean uninstall.

The local source (`/Users/whit3rabbit/Documents/GitHub/ai-hooker`, package name `agent-config`) covers a wider harness roster than synrepo currently automates — including Cline, Gemini, plus a second wave (Amp, Antigravity, CodeBuddy, Forge, Hermes, IFlow, KiloCode, OpenClaw, Qoder, Tabnine, Trae). Adopting it lets synrepo stop maintaining bespoke writers for each of those targets.

Affected synrepo modules:

- `src/bin/cli_support/commands/setup/mcp_register.rs` — six per-tool writers (~390 lines).
- `src/bin/cli_support/commands/setup/steps.rs::ensure_global_supported` — hard-coded global allow-list.
- `src/bin/cli_support/commands/setup/orchestration.rs` — multi-tool dispatch.
- `src/bin/cli_support/agent_shims/mod.rs::output_path` and `shim_content` — shim placement and content.
- `src/bin/cli_support/agent_shims/registry.rs` and `src/registry/mod.rs` — install ledger.
- `src/bootstrap/runtime_probe.rs::shim_output_path` and `src/bootstrap/report.rs::KNOWN_SHIM_PATHS` — duplicated path tables (an existing pain point flagged in `AGENTS.md`).

Stakeholders: any agent integration consumer (Claude Code is the canonical case today), operators running `synrepo setup`, anyone whose CI inspects the on-disk MCP config.

## Goals / Non-Goals

**Goals:**

- Default `synrepo setup` to a global MCP install when the harness supports it; expose an explicit `--project` opt-out.
- Replace synrepo's hand-rolled MCP/skill/instruction writers with `agent-config` calls, keyed on `owner = "synrepo"`.
- Collapse the three duplicated path tables (`agent_shims::output_path`, `runtime_probe::shim_output_path`, `report::KNOWN_SHIM_PATHS`) into a single derivation through `agent-config`.
- Preserve the doctrine block as a synrepo-owned compile-time constant; `agent-config` only places files.
- Provide a one-time migration path so installs from prior synrepo versions get adopted into the agent-config ownership ledger via `synrepo upgrade --apply`.

**Non-Goals:**

- Changing the doctrine block's wording or the agent-doctrine spec's escalation/do-not rules.
- Restructuring the `synrepo agent-setup` CLI shape, the `AgentTool` enum's visible name, or the runtime probe's detection logic.
- Adopting agent-config's `HookSpec` surface — synrepo doesn't install hooks today, and adding them is a separate change.
- Generalizing the synrepo MCP server itself to support multiple servers via the installer (the synrepo install always writes a single `synrepo` server).
- Refactoring `mcp_register.rs` into a sub-module directory before this change lands. The file shrinks from ~390 lines to ~80 lines once writers are removed; a sub-module split is unnecessary at that size.

## Decisions

**1. Add `agent-config = "0.1"` as a direct dependency of the binary crate, not the library.**

Rationale: the installer is consumed only by `src/bin/cli_support/commands/setup/`. Putting it in `Cargo.toml`'s top-level `[dependencies]` would push the dep into every consumer of `synrepo` as a library (including downstream tests and any embedding crate). The binary section's deps are scoped via `required-features` or a separate `[[bin]]` block. Concretely: list `agent-config` under `[dependencies]` (the workspace is a single crate today, so the binary and library share it), but gate adoption sites behind `cfg(feature = "...")` only if a future split makes that necessary. Alternative considered: vendor the relevant subset of `agent-config` source — rejected because the published crate has its own test surface, semver guarantees, and the maintenance cost of vendoring exceeds the dep cost.

**2. Default `synrepo setup` to `Scope::Global`; expose `--project` to opt out.**

Rationale: the user has stated the binary is now global and that "ideally it's always global." Today, `synrepo setup --global` flips the behavior and supports only four targets. The migration is to invert the default: `synrepo setup <tool>` writes globally when supported, falling back to project scope only when (a) the operator passes `--project`, or (b) the harness only supports `Scope::Local`. Alternative: keep `--global` as a non-default flag — rejected because it leaves the existing limitation in place and contradicts the stated intent. Backward compatibility: `--global` remains accepted as a no-op alias for the new default; `--project` is the new flag. The release notes call out the default flip.

**3. Tier promotion (`Automated` vs `ShimOnly`) becomes a runtime check against `agent-config`.**

Rationale: `AutomationTier` today is a hand-coded match in `src/bin/cli_support/agent_shims/mod.rs`. After adoption, the answer to "can synrepo automate MCP registration for this target?" is `agent_config::mcp_by_id(<id>).is_some()`, and "for which scope?" is `integration.supported_scopes()`. Keeping the synrepo-side enum (`AutomationTier`) only for human-readable labels in setup output is fine; the dispatch decision moves to a runtime call. Alternative: keep both lists in sync by hand — rejected because the synrepo `AGENTS.md` already calls out drift between `AutomationTier` and `step_register_mcp` dispatch, and we'd be extending the drift surface to a third list.

**4. Shim file placement uses `SkillSpec` for `SKILL.md` targets, `InstructionSpec` for the rest.**

Rationale: `agent-config` already encodes the per-harness preference (`ReferencedFile` for Claude `CLAUDE.md`-anchored instructions, `InlineBlock` for Codex `AGENTS.md`-fenced instructions, `StandaloneFile` for Cline `.clinerules/*.md`). synrepo's current `output_path` table is a partial reimplementation. We pass the doctrine body to the installer as the spec body and let agent-config own the path. The exhaustive `match` on `AgentTool` collapses to a one-line `kind` decision (`is_skill` boolean) plus a builder call. Alternative: keep synrepo's `output_path` table — rejected because of the documented three-site duplication.

**5. Owner tag is the literal string `"synrepo"`. Migration runs through `synrepo upgrade --apply`.**

Rationale: agent-config keys uninstall on `(name, owner_tag)`. Anything we install today gets `owner = "synrepo"`. Pre-existing installs (no `_agent_config_tag` JSON marker, no ledger entry) need adoption. The upgrade path: `synrepo upgrade` (dry-run) reports legacy entries; `synrepo upgrade --apply` invokes `install_*` for each, which writes the marker and ledger entry without changing the on-disk content if it already matches. If content differs from what we'd write, upgrade refuses unless the operator passes a confirmation flag. This re-uses the existing `synrepo upgrade` contract documented in `bootstrap/spec.md` rather than introducing a new top-level command.

**6. Doctrine content stays a synrepo-owned compile-time constant.**

Rationale: `AGENTS.md` is explicit that the agent-doctrine block lives in `src/bin/cli_support/agent_shims/doctrine.rs` as a `doctrine_block!()` macro and that every shim embeds it via `concat!`. Byte-identical drift tests pin the surface. Adoption MUST NOT replace that — agent-config receives the assembled body string (via `concat!`-resolved `&'static str`) as the spec body. The byte-identical test continues to compare spec bodies, not on-disk file contents alone, so it fails before any installer call when content drifts. Alternative: let agent-config own a "synrepo doctrine" template — rejected because that bypasses the compile-time enforcement and the product-boundary constraint that synrepo authors agent-facing prose.

**7. Inline-secret policy is a tripwire, not a current concern.**

Rationale: synrepo's MCP server takes no environment secrets. We pass no `env` keys to the spec today. The spec requirement around inline-secret refusal is a future-proofing tripwire so a later change that adds env keys can't accidentally smuggle secrets into a project-scoped install. Concretely: setup wraps the `install_mcp` call and surfaces `AgentConfigError::InlineSecretInLocalScope` as a setup error.

## Risks / Trade-offs

- **[Default-flip surprise]** Operators relying on the prior project-scoped default will get global writes after upgrading. → Mitigation: release-note the flip prominently, accept `--global` as a documented no-op alias for at least one minor version, and have `synrepo setup` print the resolved scope in its first line of output. The `synrepo setup` output already prints the scope today, so the visibility is already there.

- **[Adoption migration on legacy installs]** Files written by older synrepo versions don't carry the `_agent_config_tag` marker; calling `uninstall_*` on them is a no-op (correctly), but they appear "stale" until adopted. → Mitigation: `synrepo upgrade --apply` adopts them in place. `synrepo check` enumerates legacy installs as a drift class so operators see them before running upgrade. Until adopted, `synrepo remove` falls back to the legacy path-based delete logic with a deprecation warning.

- **[Transitive deps]** `agent-config` pulls in `json5`, `jsonc-parser`, `yaml_serde`, `fluent-uri`, and `sha2`. None are currently in synrepo's tree. `serde_json`, `serde`, `toml_edit`, `thiserror`, `anyhow`, `tempfile`, and `dirs` are already direct deps. → Mitigation: review the transitive footprint before merging. `agent-config` itself is `forbid(unsafe_code)`. The new transitives are well-known, MIT/Apache-2.0-licensed, and target stable Rust. Per autonomy rules in CLAUDE.md, the operator must approve the dep add before this lands.

- **[Test churn]** Existing tests in `src/bin/cli_support/tests/setup/` assert on hand-rolled JSON/TOML shapes. Most assertions about the `synrepo` MCP entry's `command`/`args` keep working because spec values are unchanged; assertions about file presence/absence and ordering may need updating. → Mitigation: keep the assertions that verify the entry shape; replace the assertions that verify "the writer wrote this file" with `install.created` / `install.patched` set checks. Add one integration test per surface (MCP, skill, instruction) that round-trips through `install` + `uninstall`.

- **[Path duplication remains during transition]** `runtime_probe::shim_output_path` and `report::KNOWN_SHIM_PATHS` are referenced from doctrine-pointer logic and runtime detection. Collapsing them to derive from agent-config is a separable cleanup. → Mitigation: this change makes them derive from a single synrepo-side helper that calls into agent-config; the three sites still exist as call sites but reference one source of truth. A follow-up change can inline them further.

- **[Sub-200-line `mcp_register.rs` becomes a thin dispatch]** The 400-line file cap in `AGENTS.md` is satisfied trivially after this change. There's no need for a sub-module directory; we could reabsorb the surface into `setup/steps.rs`. → Mitigation: keep `mcp_register.rs` as a small named module (registers MCP under owner=`"synrepo"`, returns `StepOutcome`) so call sites and tests don't all have to move.

- **[Coverage gap for harnesses without agent-config support]** If synrepo wants to support a harness agent-config doesn't yet cover (today: none — synrepo's roster is a subset of agent-config's), the fallback is a synrepo-local writer. → Mitigation: keep the trait surface small enough that adding such a writer is local; the realistic path is to upstream the harness into agent-config first.

## Migration Plan

1. **Land the dep add in a separate commit.** Operator approval per CLAUDE.md rules. Verify the lockfile diff is acceptable.
2. **Land the MCP-register rewrite behind no flag.** Existing tests that assert on JSON shapes pass because spec values are unchanged. Adds a default-global setup flow gated behind the new `--project` flag (default = global). Release-note the flip.
3. **Land the shim/skill placement rewrite.** Removes synrepo-side `output_path` duplication. Updates the doctrine byte-identical tests to compare spec bodies.
4. **Land the upgrade migration.** `synrepo upgrade` reports legacy installs; `synrepo upgrade --apply` adopts them. `synrepo check` adds a "legacy install" drift surface.
5. **Update docs (`docs/MCP.md`, `README.md`, `AGENTS.md` runtime-probe note).**

Rollback: each commit is independently revertable. The dep add is the riskiest step from an audit perspective; reverting the rewrite commits before reverting the dep is the unwind order.

## Open Questions

- **Does `agent-config 0.1` cover every harness in synrepo's `AgentTool` enum?** Spot-check before implementation: the README lists Claude, Cursor, Gemini, Codex, Copilot, OpenCode, Cline, Roo, Windsurf, plus second-wave names. synrepo lists all of those plus `Generic` (writes `synrepo-agents.md`), `Goose`, `Kiro`, `Junie`, `Qwen`. If any are missing from agent-config, those targets stay on synrepo's local writer for one transition cycle; this change still simplifies the rest.
- **Does the user want `--global` to keep working as an alias (recommended) or be removed outright?** Recommend keep-as-alias for one minor version; ask before removal.
- **Should `synrepo setup` for a `Scope::Local`-only harness (e.g., Copilot per agent-config's matrix) require explicit `--project`, or fall back automatically with a notice?** Recommend automatic fallback with a notice; the spec already allows either.
