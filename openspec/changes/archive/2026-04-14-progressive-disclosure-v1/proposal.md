## Why

Two shipping gaps remain after `synrepo-minimum-context`: agents have no way to retrieve bounded operational history from synrepo itself (reconcile outcomes, repair-log events, co-change hot files), and the `tiny/normal/deep` budget protocol is documented in the code but never explained to agents as an intentional escalation pattern. Both close the claude-mem-inspired UX gaps called out in ROADMAP.md §11.2 Phase 5.

## What Changes

- Add `synrepo_recent_activity` MCP tool returning a bounded lane of synrepo's own operational events: recent reconcile outcomes, repair-log entries, cross-link accept/reject decisions, commentary refreshes, and churn-hot files. Accepts `scope?`, `kinds?`, `limit?` (default 20, max 200), and `since?` parameters. Requires at least one of `limit` or `since`; refuses unbounded lookback.
- Add `synrepo status --recent` CLI flag surfacing the same bounded event lane from the terminal.
- Add progressive-disclosure protocol doc pass: reframe `tiny/normal/deep` as a deliberate three-surface interaction pattern (index → neighborhood → deep fetch) in `openspec/specs/cards/spec.md` and the synrepo skill bundle (`skill/SKILL.md`) so agents learn to escalate intentionally instead of defaulting to deep reads.

## Capabilities

### New Capabilities

- `recent-activity`: Bounded operational history surface over `.synrepo/state/repair-log.jsonl`, `reconcile-state.json`, and the overlay store. Defines the `synrepo_recent_activity` MCP tool contract and `synrepo status --recent` flag behavior.

### Modified Capabilities

- `mcp-surface`: Add `synrepo_recent_activity` tool registration requirement (already listed in prose but not as a formal requirement with scenarios).
- `cards`: Add progressive-disclosure protocol requirement formalizing the three-tier budget escalation pattern as an intended interaction contract, not just an implementation detail.

## Impact

- `crates/synrepo-mcp/src/main.rs`: new `synrepo_recent_activity` handler
- `src/bin/cli.rs` / `src/bootstrap/`: `synrepo status --recent` flag
- `openspec/specs/recent-activity/spec.md`: new canonical spec
- `openspec/specs/mcp-surface/spec.md`: delta (tool registration)
- `openspec/specs/cards/spec.md`: delta (progressive-disclosure requirement)
- `skill/SKILL.md`: progressive-disclosure escalation guidance
- Data sources already persisted; no new storage or schema changes required
