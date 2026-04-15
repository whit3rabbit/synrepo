## Why

Agent-facing surfaces (`skill/SKILL.md`, each `synrepo agent-setup` shim, MCP tool descriptions, and first-run bootstrap output) currently describe how to use synrepo in slightly different ways. `SKILL.md` already teaches the `tiny → normal → deep` escalation well, but the shims for cursor, copilot, codex, and windsurf emphasize different defaults and omit the product-boundary rules introduced in `docs/FOUNDATION.md` §"Product boundaries and doctrine". An agent that reads the copilot shim first and the claude shim later sees two subtly different default paths, and no surface tells the agent that synrepo is not an issue tracker, is not session memory, and does not run background behavior unless watch is explicit.

The roadmap §12 Milestone A calls this out as the first doctrinal fix. This change makes the agent workflow obvious by unifying the copy.

## What Changes

- Introduce a single canonical "agent doctrine" block that every agent-facing surface includes or references verbatim. The block covers: the default escalation path (search → `tiny` → `normal` → `deep`), the four do-not rules (no large file reads first, no treating commentary as canonical, no triggering synthesis without cause, no expecting background behavior unless watch is explicit), and the product-boundary rules (code memory not task memory, handoffs are derived, freshness explicit).
- Rewrite `skill/SKILL.md` around the unified doctrine and a single happy-path example set: unfamiliar repo orientation, "where should I edit?", change-impact inspection, then open source body.
- Rewrite each shim constant in `src/bin/cli_support/agent_shims.rs` (`CLAUDE_SHIM`, `CURSOR_SHIM`, `COPILOT_SHIM`, `GENERIC_SHIM`, `CODEX_SHIM`, `WINDSURF_SHIM`) so the doctrine block is byte-identical across targets. Target-specific text remains limited to how the target invokes MCP tools and where the shim file is written.
- Audit MCP tool descriptions in `crates/synrepo-mcp/` and ensure each card-returning tool's description names the escalation default (`tiny` first, `deep` only before edits) in one consistent sentence.
- Update first-run bootstrap output (`src/bootstrap/report.rs`) so the health-check message on success points to the same doctrine (no duplication of the full block, a one-line summary with a pointer to `synrepo-context.md` or the generated shim).
- Add a snapshot or equality test that the canonical doctrine block is present and identical across shim constants and `skill/SKILL.md`.

## Capabilities

### New Capabilities

- `agent-doctrine`: Defines the canonical agent-doctrine text, the rule that every agent-facing surface must include it verbatim (or link to it verbatim), and the test that enforces byte-equality across shims.

### Modified Capabilities

- `mcp-surface`: Add a requirement that card-returning tool descriptions name the `tiny`-first escalation default in a consistent sentence.
- `bootstrap`: Add a requirement that the first-run report points to the agent doctrine rather than restating it.

## Impact

- **Code**: New `src/bin/cli_support/agent_shims/doctrine.rs` (or `const DOCTRINE_BLOCK: &str`) holding the canonical doctrine text. Rewrites of the six shim constants in `agent_shims.rs`. Touch-ups to MCP tool description strings in `crates/synrepo-mcp/`. One-line touch in `src/bootstrap/report.rs` output copy.
- **Docs**: Rewrite of `skill/SKILL.md` sections covering escalation and do-not rules. No changes to `docs/FOUNDATION.md`, `docs/FOUNDATION-SPEC.md`, or `ROADMAP.md` (these already reflect the doctrine after 2026-04-14).
- **Specs**: New `agent-doctrine` spec; delta requirements on `mcp-surface` and `bootstrap`.
- **Risk**: Low — copy-only change plus one new constant. No storage, no pipeline, no schema changes.
