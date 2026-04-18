## Context

`synrepo agent-setup` started with three agents in the `Automated` tier because Claude, Codex, and OpenCode were our initial target platforms. As more agents were added (Cursor, Windsurf, Gemini, Goose, Kiro, Qwen, Junie, Roo, Tabnine, Trae), they all landed in `ShimOnly`. That tier is the correct placement for agents without a documented project-scoped MCP config — Gemini CLI, for example, reads a TOML command file, not an MCP registry. But Cursor, Windsurf, and Roo all have documented project-scoped MCP JSON configs in the same shape as Claude's `.mcp.json`. Leaving them in `ShimOnly` is purely a historical oversight.

The existing `automation_tier_matches_step_register_mcp_dispatch` test enforces that `AgentTool::automation_tier() == Automated` iff `step_register_mcp` has a specific `match` arm for the target. Promoting an agent without writing the MCP function breaks CI — which is exactly the safety net we want.

## Goals / Non-Goals

**Goals:**

- Eliminate the "register MCP server manually" message for Cursor, Windsurf, and Roo.
- Preserve idempotency and "refuse to overwrite non-JSON" behavior from the existing Claude writer.
- Do not touch agents that legitimately belong in `ShimOnly`.

**Non-Goals:**

- No generalization of the three new writers into a single helper. The Claude writer is 50 lines and so will each of these be; a shared abstraction saves 30 lines and adds a layer of indirection. If a fourth JSON-based agent ships later, revisit.
- No change to the shim doctrine block — the core behavioral rules are unchanged.
- No change to user-level configuration files (`~/.codeium/windsurf/mcp_config.json`, etc.). Project scope only.

## Decisions

### D1: Three separate writer functions, not one parameterized helper

Claude, Cursor, Windsurf, and Roo all use the same shape:

```json
{
  "mcpServers": {
    "synrepo": {
      "command": "synrepo",
      "args": ["mcp", "--repo", "."]
    }
  }
}
```

A naive helper would be:

```rust
fn setup_json_mcp(repo_root: &Path, rel_path: &Path) -> anyhow::Result<StepOutcome> { ... }
```

Cost: a new helper that the test suite has to exercise per-caller to be sure the caller wired it correctly. Benefit: ~30 lines saved.

Decision: **duplicate** the writer. `setup.rs` becomes slightly longer (from ~650 lines to ~800). Each writer is easy to read in isolation, matches the Claude test pattern exactly, and does not add a shared code path whose failure modes would be hidden from the per-agent tests.

If a fifth agent with the same shape arrives (Windsurf Pro? Aider?), revisit.

### D2: Windsurf writes `.windsurf/mcp.json`, not the global config

Windsurf supports both `~/.codeium/windsurf/mcp_config.json` (user scope) and `.windsurf/mcp.json` (project scope). `synrepo setup` runs in a specific repo context; project scope is correct. Operators who want to use synrepo globally can symlink or copy; that's a user decision, not a synrepo one.

Claude's writer uses the same reasoning: it writes `.mcp.json` at the repo root, not `~/.claude/claude_desktop_config.json`.

### D3: Roo's config file is `.roo/mcp.json`, not `.roo/commands/synrepo.md`

The shim is already written to `.roo/commands/synrepo.md` (agent_shims/mod.rs:158). The MCP config is a separate concern and lives in `.roo/mcp.json`. These are two different files that both need to exist for the automated flow to work. The shim teaches the agent how to use synrepo; the MCP config is what lets the agent connect.

### D4: Shim copy update

Each shim's "CLI fallback" section currently reads something like:

> The `synrepo mcp --repo .` server should be registered in your agent's MCP config. Refer to your agent's docs for the exact path.

Update the three promoted shims to read:

> The `synrepo mcp --repo .` server is registered in your project's MCP config automatically by `synrepo setup`. Tools available: …

The existing test `every_shim_embeds_doctrine_block` (`agent_shims/tests.rs`) covers the byte-identical doctrine region. The "CLI fallback" section is outside that region, so the edit is free.

### D5: Tests

Per writer, three cases following `setup_claude_mcp_*` patterns:

- `setup_cursor_mcp_creates_config_when_missing`
- `setup_cursor_mcp_idempotent_on_rerun`
- `setup_cursor_mcp_merges_into_existing_entries`
- `setup_cursor_mcp_refuses_invalid_json`

Same four for `windsurf` and `roo`. Total: 12 new tests.

The `automation_tier_matches_step_register_mcp_dispatch` test already exists; it will auto-validate that the tier-move and dispatch-extend agree after both are done.

## Risks / Trade-offs

- **Cursor / Windsurf / Roo might change their MCP config format**. Low risk — the JSON shape is simple and shared across the ecosystem (follows Claude's). If one of them diverges, the writer for that agent needs an update, and the existing test suite will catch the mismatch on first run.

- **Already-existing user configs could have idiosyncratic layouts**. The `load_json_config` helper handles this: parse-or-fail-loud on invalid JSON, merge on valid JSON. Existing users' `.cursor/mcp.json` with custom `mcpServers.other_server` entries will survive; only `mcpServers.synrepo` is written. Same behavior as Claude's writer, proven by the `setup_claude_mcp_merges_into_existing_entries` test.

- **Path collisions**: no two tier changes share a config file. `.cursor/mcp.json`, `.windsurf/mcp.json`, `.roo/mcp.json` are all distinct from the shim output paths.

## Migration Plan

Single PR. No data migration. The change is strictly additive from the user's perspective: agents that were `ShimOnly` still work (shim is still written; operator can still wire MCP manually if they ignore the automated step). Three agents just gain an additional automated step.

Rollback: revert the tier assignment and delete the three new writer functions. No user data is affected — the `.cursor/mcp.json` / `.windsurf/mcp.json` / `.roo/mcp.json` files persist but are no longer rewritten by synrepo.

## Open Questions

- **O1**: Should the writer handle existing non-synrepo entries under `mcpServers` by preserving them? Yes — the Claude writer already does this. Inherit the behavior.
- **O2**: Should we promote Copilot as well? Probably not in this change — Copilot's MCP config lives in VS Code settings rather than a project-scoped file, so it requires a different pattern. Keep Copilot in `ShimOnly` for now; a future change can tackle it.
- **O3**: Should we also promote Gemini CLI? No — Gemini reads a TOML command file (`.gemini/commands/synrepo.toml`), not an MCP registry. Different pattern; out of scope.
