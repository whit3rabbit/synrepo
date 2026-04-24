## Why

Synrepo now exposes workflow aliases, but the product promise depends on agents actually following the bounded-context loop. This change hardens the doctrine, tool descriptions, and metrics around "ask synrepo first, read files only after card routing."

## What Changes

- Tighten agent doctrine around orient, find, impact or risks, edit, tests, and changed-context checks.
- Update MCP tool descriptions and info text so clients see the same workflow guidance.
- Add cold-file-read avoidance and escalation behavior to context-accounting metrics where observable.
- Require full-file reads to be framed as an explicit escalation after card routing, not the default first step.
- Keep the rule advisory and measurable; synrepo does not intercept arbitrary filesystem reads by external agents.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `agent-doctrine`: strengthens workflow rules and full-file-read escalation language.
- `mcp-surface`: tool descriptions and server info expose the workflow contract.
- `context-accounting`: metrics include workflow-adherence signals where synrepo can observe them.
- `minimum-context`: reinforces minimum-context as the bounded inspection step before deeper reads.

## Impact

- Generated shims and canonical doctrine text.
- MCP tool descriptions and `get_info` text.
- Context metrics fields or derived counters.
- Tests for doctrine byte identity and tool-description consistency.
