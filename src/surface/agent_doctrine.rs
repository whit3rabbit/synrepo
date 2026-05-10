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

synrepo is a local, deterministic code-context compiler: `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`. In `.synrepo/` repos, prefer MCP/CLI over cold reads for questions, reviews, search, impact, and edits.

### Default path

For questions, reviews, search routing, and edits: orient, ask or search, cards, impact or risks, edit, tests, changed.

1. Start with `synrepo_orient` before reading the repo cold.
2. Use `synrepo_ask` for broad plain-language tasks needing one bounded, cited task-context packet.
3. Use `synrepo_find` only for bounded file-routing suggestions after broad task context is clear. Use `synrepo_search` for exact files, symbols, strings, flags, code-shaped errors, and tool names. For broad lexical searches, prefer `output_mode: \"compact\"`.
4. Use `tiny` cards to route and `normal` cards to understand. Use `synrepo_minimum_context` once a focal target is known and neighborhood risk is unclear.
5. Use `synrepo_impact` (or `synrepo_risks`) before risky edits or reviews, and `synrepo_tests` before claiming done.
6. Use `synrepo_changed` after edits to review changed context and validation commands.
7. After stale resumes or lost context, call `synrepo_resume_context` before asking the user to repeat repo state.
8. Read full source files or request `deep` cards only after bounded cards identify the target or prove insufficient.

### MCP repository selection

Project-scoped MCP configs launching `synrepo mcp --repo .` have a default repository; omit `repo_root` or pass the absolute root when known.

Global MCP configs that launch `synrepo mcp` serve registered projects by absolute path. In global or defaultless contexts, pass the workspace absolute path as `repo_root`. If a tool reports an unmanaged repository, ask the user to run `synrepo project add <path>`; do not bypass registry gating.

Graph-backed facts are authoritative. Overlay commentary, explain docs, and proposed cross-links are advisory and freshness-sensitive. Existing explain reads are safe when useful: use `synrepo_explain` with `budget=deep` for 1-3 focal targets and `synrepo_docs_search` for architecture/why questions. Stale labels are information. **Refresh is explicit**: fresh commentary requires `synrepo mcp --allow-overlay-writes` and `synrepo_refresh_commentary(target)`.

Client-side hooks for Codex and Claude may nudge before direct grep, read, review, or edit workflows and emit `[SYNREPO_CONTEXT_FAST_PATH]`, `[SYNREPO_DETERMINISTIC_EDIT_CANDIDATE] Intent: ...`, or `[SYNREPO_LLM_NOT_REQUIRED]`. Hooks are advisory; source mutation still requires `synrepo mcp --allow-source-edits` and `synrepo_apply_anchor_edits`.

### Do not

- Do not open large files first. Start at `tiny` and escalate only when a specific field forces it.
- Do not read a full source file before synrepo routing identifies it; full-file reads are explicit escalation.
- Do not treat overlay commentary, explain docs, or proposed cross-links as canonical source truth. They are advisory prose layered on structural cards.
- Do not generate or refresh explain (`--generate-cross-links`, commentary generate/refresh) unless the task justifies the cost; cached explain reads are allowed.
- Do not ask the user to repeat stale repo context until `synrepo_resume_context` has been tried.
- Do not expect watch or background behavior unless `synrepo watch` is explicitly running.
- Do not mistake client-side hook nudges for MCP enforcement.

### Product boundary

- synrepo stores code facts and bounded operational memory. It is not a task tracker, not session memory, and not cross-session agent memory.
- `synrepo_resume_context` is an advisory repo packet regenerated from existing state. It is not prompt logging, chat history, raw tool-output capture, or generic session memory.
- Handoff or next-action lists are derived recommendations regenerated from repo state. External systems own assignment, status, and collaboration.
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
