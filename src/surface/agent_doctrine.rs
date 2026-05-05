//! Canonical agent doctrine for every synrepo-facing surface.
//!
//! The macros `doctrine_block!` and `tool_desc_escalation_line!` expand to
//! string literals so they can be embedded by `concat!` at compile time in
//! agent shims and MCP tool description attributes. The [`DOCTRINE_BLOCK`] and
//! [`TOOL_DESC_ESCALATION_LINE`] constants expose the same text as `&'static
//! str` for runtime checks.
//!
//! Source-of-truth prose lives in `docs/FOUNDATION.md` §"Product boundaries
//! and doctrine"; the macros mirror that text and are consumed by the
//! `synrepo` binary (agent-setup shims, `synrepo mcp` tool descriptions),
//! and `skill/SKILL.md` (via a runtime assertion test).

/// Canonical doctrine text, Markdown-formatted, heading `## Agent doctrine`.
///
/// Must be a macro so that `concat!` sites (shim constants, MCP tool
/// description attributes) can embed it at compile time.
#[macro_export]
macro_rules! doctrine_block {
    () => {
"## Agent doctrine

synrepo is a code-context compiler. When `.synrepo/` exists in the repo root, prefer MCP tools (or the CLI fallback) over cold file reads for orientation, codebase questions, file reviews, broad search, change impact, and pre-edit context.

### Default path

The required sequence for codebase questions, reviews, search routing, and edits is orient, find, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_find` or `synrepo_search` to find candidate files and symbols. `synrepo_find` decomposes broad task language into deterministic lexical anchors before returning empty; for broad lexical searches, prefer `output_mode: \"compact\"` so results are grouped and token-accounted before opening files.
3. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` once a focal target is known but the surrounding neighborhood risk is unclear, especially for file reviews and codebase questions.
4. Use `synrepo_impact` (or its shorthand `synrepo_risks`) before editing or reviewing risky files, and `synrepo_tests` before claiming done.
5. Use `synrepo_changed` after edits to review changed context and validation commands.
6. Read full source files or request `deep` cards only after bounded cards identify the target or when the card content is insufficient. Full-file reads are an explicit escalation, not the default first step.

### MCP repository selection

Project-scoped MCP configs that launch `synrepo mcp --repo .` have a default repository, so `repo_root` may be omitted. Passing the absolute repository root is still valid and preferred when you can identify it reliably.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the current workspace's absolute path as `repo_root` to repo-addressable tools. If a tool reports that a repository is not managed by synrepo, ask the user to run `synrepo project add <path>`; do not bypass registry gating.

Graph-backed structural facts (files, symbols, edges) remain the authoritative source of truth. Overlay commentary, explain docs, and proposed cross-links are advisory, labeled machine-authored, and freshness-sensitive. Treat stale labels as information, not as errors. **Refresh is explicit**: every tool returns what is currently in the overlay. Fresh commentary refresh requires `synrepo mcp --allow-overlay-writes` and `synrepo_refresh_commentary(target)`.

Client-side nudge hooks for Codex and Claude may remind agents to use synrepo before direct grep, read, review, or edit workflows and emit `[SYNREPO_CONTEXT_FAST_PATH]`, `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: ...`, or `[SYNREPO_LLM_NOT_REQUIRED]`. Hooks are advisory only; source mutation still requires `synrepo mcp --allow-source-edits` and `synrepo_apply_anchor_edits`.

### Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not read a full source file before synrepo routing has identified it; treat a full-file read as an escalation after the bounded card is insufficient.
- Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth. They are advisory prose layered on structural cards.
- Do not trigger explain (`--generate-cross-links`, deep commentary refresh) unless the task justifies the cost.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
- Do not mistake client-side hook nudges for MCP interception or enforcement. They are non-blocking reminders.

### Product boundary

- synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
- Any handoff or next-action list is a derived recommendation regenerated from repo state. External task systems own assignment, status, and collaboration.
- Freshness is explicit. A stale label is information, not an error; it is not silently refreshed on read.
"
    };
}

/// One-sentence escalation default appended to card-returning MCP tool
/// descriptions. Tied to `doctrine_block!` so the wording cannot drift per
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
