## Context

The current dashboard answers whether synrepo is initialized, watching, and generally healthy. Context metrics and overlay-note lifecycle data now exist, but the dashboard does not yet frame them as a trust contract for agents and users.

## Goals / Non-Goals

**Goals:**

- Show bounded-context effectiveness: cards served, average tokens, tokens avoided, stale responses, and truncation or escalation signals.
- Show advisory memory health: active, stale, unverified, superseded, forgotten, and invalid note counts.
- Show current-change risk shape with bounded changed-file, symbol, test, and risk summaries.
- Keep dashboard state read-only and sourced from existing snapshots or bounded queries.

**Non-Goals:**

- No interactive benchmark runner in the dashboard.
- No generic agent transcript or prompt logging.
- No hidden overlay-note promotion into graph truth.
- No dashboard-hosted MCP server.

## Decisions

1. **Add a Trust view rather than overloading Health.** Health remains quick operational readiness. Trust focuses on context quality, freshness, and advisory surfaces.

2. **Prefer aggregate rows with bounded samples.** Counts and status rows come first; samples are capped and used only to help the operator decide the next action.

3. **Use repair-loop recommendations for action text.** The dashboard should not invent remediation policy when `check` and `sync` already own stale-surface classification.

4. **Treat missing metrics as no-data, not zero.** A fresh repo with no card traffic should not claim zero stale responses as a proven healthy usage history.

## Risks / Trade-offs

- Too many rows can make the TUI noisy, mitigation: group trust rows by context, overlays, and change impact.
- Current-change impact depends on available git or reconcile data, mitigation: show degraded or unavailable states explicitly.
- Aggregate-only data can hide a bad sample, mitigation: include bounded recent activity samples where existing data supports it.
