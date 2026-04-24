## 1. MCP workflow alias parity

- [x] 1.1 Register `synrepo_risks` in `src/bin/cli_support/commands/mcp.rs` as an alias of `synrepo_impact` (same `ChangeRiskParams`, same `cards::handle_change_risk` handler).
- [x] 1.2 Update the server `get_info` instructions string to name `synrepo_risks` alongside `synrepo_impact`.
- [x] 1.3 Update `src/surface/agent_doctrine.rs` so the generated doctrine names `synrepo_risks` next to `synrepo_impact`.

## 2. Dashboard Health rows

- [x] 2.1 Extend `src/tui/probe/mod.rs::build_health_vm` to push a `tokens avoided` row after the existing `context` row, reading `metrics.estimated_tokens_saved_total`.
- [x] 2.2 Push a `stale responses` row reading `metrics.stale_responses_total`, with `Severity::Stale` when the count is greater than zero.
- [x] 2.3 Update probe snapshot tests to cover the new rows.

## 3. Verification

- [x] 3.1 `cargo test --bin synrepo mcp` (MCP tool registration and source scans).
- [x] 3.2 `cargo test --lib tui::probe` (new rows visible).
- [x] 3.3 `make check` passes (fmt + lint + workspace tests).
- [x] 3.4 `openspec status --change context-workflow-follow-ups-v1 --json` shows `isComplete: true`.
