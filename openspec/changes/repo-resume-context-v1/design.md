## Context

Synrepo already exposes the ingredients for a resume packet: `synrepo_changed`, `synrepo_next_actions`, `synrepo_recent_activity`, explicit overlay notes, and aggregate context metrics. Today agents must call several tools and assemble a continuation view themselves, which wastes context and increases the chance they ask the user to repeat repo state.

The packet must preserve synrepo's trust boundary. It is derived, advisory, and regeneratable. Graph facts remain authoritative, overlay notes remain explicit saved-context writes, and hooks remain non-blocking nudges.

## Goals / Non-Goals

**Goals:**
- Provide one explicit CLI/MCP surface that returns the smallest useful repo-resume packet.
- Include pointers to existing detail surfaces instead of embedding broad history.
- Enforce a response token cap by dropping lower-priority sections first.
- Track only aggregate context metrics for the new response.

**Non-Goals:**
- No automatic session capture, PreCompact/SessionStart restore, prompt logging, chat history, or raw tool-output persistence.
- No new storage schema.
- No graph or overlay schema changes.
- No promotion of overlay notes into graph-backed truth.

## Decisions

1. **Add a dedicated surface module.** The collector should live outside MCP-specific code so CLI and MCP share one implementation. Alternative: implement only in MCP. Rejected because the CLI command is part of the public interface and tests should exercise one shared contract.

2. **Reuse existing sources directly.** Changed files should use an extracted shared helper from the current MCP changed-context code. Next actions should call `collect_handoffs`; recent activity should call `read_recent_activity`; notes should call `SqliteOverlayStore::query_notes` and `note_counts_impl`. Alternative: route through MCP handlers internally. Rejected because stringified MCP output is harder to validate and budget.

3. **Summarize notes, do not inline note history.** The packet includes note id, target, lifecycle status, confidence, updated time, and a short claim preview. Full claims stay in `synrepo_notes`. Alternative: include full note bodies. Rejected because this would make resume-context a memory dump.

4. **Trim by section priority.** Header, context state, validation commands, changed files, and detail pointers are retained before handoffs, note summaries, recent activity, and metrics hints. Alternative: truncate strings blindly. Rejected because JSON should remain valid and high-value continuation state should survive tight budgets.

## Risks / Trade-offs

- **[Risk] The packet is mistaken for session memory.** -> Mitigation: docs and output label it as repo-scoped, advisory, and derived from existing sources only.
- **[Risk] Note previews leak too much saved context.** -> Mitigation: fixed preview length and full note details only through explicit note queries.
- **[Risk] Budget trimming hides useful recent activity.** -> Mitigation: `omitted` records section/item drops and `detail_pointers` show exact follow-up calls.
