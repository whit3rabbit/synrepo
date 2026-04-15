//! Canonical agent doctrine block.
//!
//! Every agent-facing surface (shim, SKILL.md, MCP card-returning tool
//! descriptions, bootstrap success pointer) carries this text verbatim or
//! references a compile-time constant tied to it. The source-of-truth prose
//! lives in `docs/FOUNDATION.md` §"Product boundaries and doctrine"; this
//! constant mirrors it.
//!
//! Shim constants embed the block via the `doctrine_block!` macro so the
//! byte-identical property is enforced at compile time, not by a runtime
//! snapshot test. Edits made here propagate to every shim on the next
//! `cargo build`.

/// Canonical doctrine text, Markdown-formatted, heading `## Agent doctrine`.
///
/// The macro form is required because `concat!` only accepts literal tokens.
/// `doctrine_block!()` expands to the same string literal everywhere it is
/// invoked.
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

Overlay commentary and proposed cross-links are advisory, labeled machine-authored, and freshness-sensitive. Pass `require_freshness=true` only when freshness actually matters for the task.

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

pub(crate) use doctrine_block;

// Consumed by tests that enforce byte-identical doctrine inclusion across
// shims; the shim constants themselves use the `doctrine_block!()` macro to
// embed this text at compile time, not this const, so outside of tests the
// symbol looks dead.
#[allow(dead_code)]
pub(crate) const DOCTRINE_BLOCK: &str = doctrine_block!();

// Pending MCP wiring (agent-doctrine-v1 task 4): card-returning MCP tool
// descriptions will append this sentence so escalation wording does not drift
// per tool.
#[allow(dead_code)]
pub(crate) const TOOL_DESC_ESCALATION_LINE: &str =
    "Default budget is tiny; escalate to normal for local understanding and deep only before edits.";
