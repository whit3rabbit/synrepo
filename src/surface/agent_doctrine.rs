//! Canonical agent doctrine for every synrepo-facing surface.
//!
//! The macros [`doctrine_block!`] and [`tool_desc_escalation_line!`] expand to
//! string literals so they can be embedded by `concat!` at compile time in
//! agent shims and MCP tool description attributes. The [`DOCTRINE_BLOCK`] and
//! [`TOOL_DESC_ESCALATION_LINE`] constants expose the same text as `&'static
//! str` for runtime checks.
//!
//! Source-of-truth prose lives in `docs/FOUNDATION.md` §"Product boundaries
//! and doctrine"; the macros mirror that text and are consumed by the
//! `synrepo` binary (agent-setup shims), `synrepo-mcp` (card-returning tool
//! descriptions), and `skill/SKILL.md` (via a runtime assertion test).

/// Canonical doctrine text, Markdown-formatted, heading `## Agent doctrine`.
///
/// Must be a macro so that `concat!` sites (shim constants, MCP tool
/// description attributes) can embed it at compile time.
#[macro_export]
macro_rules! doctrine_block {
    () => {
"## Agent doctrine

synrepo is a code-context compiler. When `.synrepo/` exists in the repo root, prefer MCP tools (or the CLI fallback) over cold file reads for orientation and navigation.

### Default path

1. Start with search or entry-point discovery to find candidates.
2. Use `tiny` cards to orient and route.
3. Use `normal` cards to understand a neighborhood.
4. Use `deep` cards only before writing code, or when exact source or body details matter.

Overlay commentary and proposed cross-links are advisory, labeled machine-authored, and freshness-sensitive. Treat stale labels as information, not as errors. Request fresh synthesis explicitly only when the task actually needs it.

### Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not treat overlay commentary as canonical. It is advisory prose layered on structural cards.
- Do not trigger synthesis (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.

### Product boundary

- synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
- Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed.
"
    };
}

/// One-sentence escalation default appended to card-returning MCP tool
/// descriptions. Tied to [`doctrine_block!`] so the wording cannot drift per
/// tool.
#[macro_export]
macro_rules! tool_desc_escalation_line {
    () => {
        "Default budget is tiny; escalate to normal for local understanding and deep only before edits."
    };
}

/// Runtime handle on the canonical doctrine block.
pub const DOCTRINE_BLOCK: &str = doctrine_block!();

/// Runtime handle on the escalation-default sentence.
pub const TOOL_DESC_ESCALATION_LINE: &str = tool_desc_escalation_line!();
