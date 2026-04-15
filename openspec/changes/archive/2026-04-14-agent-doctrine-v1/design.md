## Context

Today the agent-facing surfaces carry overlapping but drifting copy:

- `skill/SKILL.md` (222 lines) gives the most complete picture: a detect-first check, a card/tool reference, a full `tiny/normal/deep` protocol section with a rule of thumb and escalation playbook, and several worked examples. It reads like the canonical source.
- `src/bin/cli_support/agent_shims.rs` holds six `&'static str` shims (`CLAUDE_SHIM` at line 91ish, then one per target), each describing the MCP tool set and the budget tiers in its own words. Budget language is close but not identical across targets.
- MCP tool descriptions in `crates/synrepo-mcp/` include one-line summaries per tool but do not consistently mention the escalation default.
- `src/bootstrap/report.rs` emits first-run health output but does not direct the reader to the doctrine.

The user-visible effect is that an agent reading only the copilot shim will not be told to start at `tiny`; an agent reading only the cursor shim will not be told that overlay commentary is advisory; no shim mentions the product-boundary rules at all. The `docs/FOUNDATION.md` "Product boundaries and doctrine" section added on 2026-04-14 is the new source-of-truth, but it is not reflected in agent copy.

The fix is a single canonical doctrine block, included verbatim across every agent-facing surface, with target-specific text limited to invocation details.

## Goals / Non-Goals

**Goals:**

- Every agent-facing surface (six shims, `skill/SKILL.md`, bootstrap report, MCP card-returning tool descriptions) describes the default path the same way.
- A test proves the doctrine block is byte-identical across the six shim constants and `skill/SKILL.md`.
- New users running `synrepo init && synrepo agent-setup <tool>` get the same escalation rules regardless of which `<tool>` they chose.
- The product-boundary rules (code memory not task memory, handoffs are derived, no background behavior without explicit watch) are visible in every shim and in `SKILL.md`.

**Non-Goals:**

- No changes to the MCP protocol, tool contracts, or response shapes. Only tool descriptions change.
- No new CLI commands, no new flags, no schema or storage changes.
- No changes to `docs/FOUNDATION.md`, `docs/FOUNDATION-SPEC.md`, or `ROADMAP.md`. Those already landed on 2026-04-14 and are the source the shims borrow from.
- `AGENTS.md` (contributor doc, not agent-facing) stays as-is.

## Decisions

### 1. Canonical doctrine lives in a Rust constant, not a separate file

`src/bin/cli_support/agent_shims.rs` already owns all shim strings. Add a `pub(crate) const DOCTRINE_BLOCK: &str = "..."` in the same module (or a small `doctrine.rs` submodule if the file grows near the 400-line threshold). Each shim constant embeds `DOCTRINE_BLOCK` via `concat!` so the byte-identical property is enforced at compile time, not by snapshot test.

**Alternative considered**: Ship the doctrine as a Markdown file under `skill/` and include it at build time via `include_str!`. Rejected because it splits ownership: the compile-time `concat!` approach keeps the doctrine discoverable next to the shims and makes the byte-identical guarantee trivially provable.

### 2. `skill/SKILL.md` includes the doctrine as a prose section, not via `include`

SKILL.md is Markdown consumed by tooling that reads the file directly; it cannot `include` a Rust string. The doctrine text is duplicated there, and a test reads both `skill/SKILL.md` and `DOCTRINE_BLOCK` and asserts the expected paragraph appears in SKILL.md. This is the only surface where the block is textually duplicated; keeping it syntactically in prose avoids breaking Markdown rendering.

### 3. Bootstrap report gets a one-line pointer, not the full block

First-run output is already dense. Adding a 30-line doctrine block would drown the health summary. Instead the bootstrap report prints a single line on success: `"Agent doctrine: tiny → normal → deep. See <shim-path> for the full protocol."` This is enough to route the user to the shim the `agent-setup` command writes. A unit test checks the line appears when health is OK.

### 4. MCP tool descriptions stay short

The MCP protocol's tool `description` field is surfaced in tool-listing responses and is often rendered inline in agent UIs. A full doctrine block would bloat every response. Each card-returning tool description gets one new sentence in the same shape: `"Default budget is tiny; escalate to normal for local understanding and deep only before edits."` That sentence is a `const` imported from `agent_shims` so the tool-description source is also tied to the canonical block, preventing drift.

### 5. Boundary rules travel with the escalation rules

The product-boundary paragraph (code memory not task memory, handoffs derived, no background behavior without explicit watch) lives in the same `DOCTRINE_BLOCK`. Splitting the two would let the escalation rules propagate without the boundary rules, which is exactly the drift this change is trying to stop.

## Canonical doctrine block (proposed text)

Literal content is finalized in tasks §1; the block is roughly 25 lines and contains:

1. One-line synrepo identification ("synrepo is a code-context compiler; use the MCP tools, not cold file reads, for orientation and navigation").
2. Default path: search or entry-point discovery → `tiny` → `normal` → `deep`, with the rule "`deep` only before edits or when exact source/body details matter".
3. Overlay rule: commentary and proposed links are advisory, labeled machine-authored, and freshness-sensitive; pass `require_freshness=true` only when it matters.
4. Do-not rules (4 bullets): no large file reads first, no treating commentary as canonical, no triggering synthesis without cause, no expecting background behavior unless watch is explicit.
5. Product boundary (3 bullets): synrepo stores code facts, not tasks; handoffs are recommendations, not canonical planning state; external task systems own assignments and status.

## Risks / Trade-offs

- **Block length in MCP responses.** MCP tool descriptions stay a single sentence; the full block lives only in shims and SKILL.md. Adding the full block to MCP would inflate every `list-tools` response.
- **Copy rot.** Once the block is stabilized, changes must be made in one place (`DOCTRINE_BLOCK`) and propagated to `skill/SKILL.md` by the test fail-loud mechanism. If the test detects divergence, CI blocks the merge.
- **Shim file size.** `agent_shims.rs` is already 17.3 KB. Embedding a 25-line block six times (via `concat!`) adds only a handful of lines of code (the block itself is shared); the compiled binary grows by the block size times six, which is negligible.
- **Target-specific text shrinks.** Some shim-specific content (cursor's MDC frontmatter requirements, windsurf's rule directory conventions) is non-negotiable and stays. The change reduces drift on shared concepts, not target-specific concerns.

## Out of scope, captured for later

- Handoff surface copy (`synrepo_next_actions` / `synrepo handoffs`) — not shipped yet, will be added to the doctrine when Milestone B lands.
- Compaction copy (`synrepo compact`) — same, waits on Milestone D.
- Auto-regeneration on `docs/FOUNDATION.md` change — not worth automating until the doctrine changes more than once a year.
