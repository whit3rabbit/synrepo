## Context

synrepo has three relevant trust lanes today:

- Canonical graph facts are deterministic and source-derived.
- Overlay commentary is machine-authored advisory content.
- Overlay cross-link candidates are evidence-scored suggestions that require explicit review before promotion.

Agent notes are a fourth surface: explicit observations written by an agent or user during work. They can preserve useful local knowledge, but they are not source facts. The design must let agents remember useful observations without weakening the graph model.

## Goals / Non-Goals

**Goals:**
- Store agent observations as overlay-only advisory records with strong provenance.
- Make lifecycle state visible: active, stale, superseded, forgotten, invalid, and unverified.
- Invalidate notes when the source facts they cite drift.
- Allow notes to link to files, symbols, concepts, tests, cards, and other notes.
- Keep note retrieval explicit, bounded, labeled, and auditable.

**Non-Goals:**
- No automatic chat-history capture.
- No generic session memory.
- No LLM consolidation of graph facts, symbol facts, test mappings, dependency edges, or security findings.
- No mutation of `graph::Epistemic` or canonical graph nodes/edges.
- No implicit use of notes as synthesis input for structural cards.
- No source graph changes, no generic session memory, and no implicit use of notes as structural-card input.

## Decisions

1. **Use a separate note record type in the overlay store.** Agent notes are stored beside overlay content, not in graph tables. They use overlay epistemics and are always returned with `source_store: "overlay"` and `advisory: true`.

2. **Require a target and a claim.** A note must identify what it is about and what it asserts. Targets may be node IDs, repo paths, test IDs, card target names, or another note. Free-floating memories are rejected.

3. **Require provenance on write.** A valid note includes `created_by`, `created_at`, `target`, `claim`, `evidence`, `confidence`, and either `source_hashes` or a graph revision anchor. Missing required provenance makes the note invalid for normal card surfaces.

4. **Model lifecycle as append-only transitions.** `supersede`, `forget`, `verify`, and `invalidate` create auditable transitions rather than silently overwriting history. Normal retrieval hides forgotten notes by default but audit queries can include tombstones.

5. **Invalidate on source drift.** Notes that cite source hashes, graph revisions, or evidence spans become stale when those anchors no longer match current source-derived facts. Stale notes remain inspectable but must not be surfaced as fresh.

6. **Apply decay only to notes.** Notes can decay or require re-verification after time or drift. AST facts, file hashes, imports, call graph edges, test mappings, and git-derived ownership facts do not decay.

7. **Keep card integration opt-in and labeled.** Structural cards do not silently include notes. A caller must request notes, or use a budget tier/tool that explicitly documents note inclusion. Included notes live under a distinct advisory field and do not alter structural fields.

8. **Expose explicit operations.** MCP tools are `synrepo_note_add`, `synrepo_note_link`, `synrepo_note_supersede`, `synrepo_note_forget`, `synrepo_note_verify`, and `synrepo_notes`. CLI commands live under `synrepo notes`: `add`, `list`, `link`, `supersede`, `forget`, `verify`, and `audit`.

9. **Retain audit history by default.** Lifecycle transitions and forgotten-note tombstones are retained for audit. Normal retrieval hides forgotten notes. Future physical pruning must be explicit and preserve trust semantics by retaining an audit summary.

## Proposed Record Shape

```json
{
  "note_id": "note_...",
  "kind": "agent_observation",
  "target": {
    "kind": "path",
    "id": "src/pipeline/repair/report.rs"
  },
  "claim": "This file formats repair-loop findings but does not execute fixes.",
  "evidence": [
    { "kind": "symbol", "id": "symbol_..." },
    { "kind": "test", "id": "repair_report_formats_findings" }
  ],
  "confidence": "medium",
  "created_by": "codex",
  "created_at": "2026-04-24T00:00:00Z",
  "updated_at": "2026-04-24T00:00:00Z",
  "source_hashes": ["..."],
  "graph_revision": "rev_...",
  "status": "active",
  "expires_on_drift": true,
  "supersedes": [],
  "superseded_by": null,
  "verified_at": null,
  "verified_by": null,
  "invalidated_by": null,
  "source_store": "overlay",
  "advisory": true
}
```

## Lifecycle States

- `active`: valid provenance and no known drift.
- `unverified`: valid shape, but caller supplied no evidence or evidence has not been checked.
- `stale`: source hashes, graph revision, or evidence spans no longer match current source facts.
- `superseded`: a newer note replaces this claim.
- `forgotten`: intentionally hidden from normal retrieval by tombstone.
- `invalid`: missing required provenance or malformed target/evidence.

## Surfaces

- MCP tools: `synrepo_note_add`, `synrepo_note_link`, `synrepo_note_supersede`, `synrepo_note_forget`, `synrepo_note_verify`, `synrepo_notes`.
- CLI commands: `synrepo notes add`, `synrepo notes list`, `synrepo notes verify`, `synrepo notes forget`, `synrepo notes audit`.
- Status/dashboard fields: active notes, stale notes, unverified notes, forgotten tombstones, drift-invalidated notes.
- Repair/check integration: report note drift and recommend reverify or forget.

## Risks / Trade-offs

- **Agents can over-trust notes:** every surfaced note must be advisory, source-labeled, and separated from graph-backed fields.
- **Note spam can degrade retrieval:** require targets, enforce bounded listing, and prefer explicit note tools over automatic capture.
- **Privacy can be weakened by prompt capture:** do not store chat prompts, hidden chain-of-thought, or full session transcripts.
- **Drift can make old advice harmful:** notes should stale on source drift and re-enter normal surfaces only after verification.
- **Audit history can grow:** tombstones and transitions may need retention controls, but physical pruning must preserve user intent and trust semantics.
