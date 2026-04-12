## Context

Milestone 3 shipped the five core MCP tools (`synrepo_overview`, `synrepo_card`, `synrepo_search`, `synrepo_where_to_edit`, `synrepo_change_impact`), plus compiled `SymbolCard` and `FileCard` responses. `ModuleCard` exists only as a struct placeholder today. The graph now contains FileNodes, SymbolNodes, and ConceptNodes produced by stage 3 prose extraction. `EdgeKind::Governs` is defined in the type system but not yet emitted by any pipeline stage. Stage 3 (`parse_prose`) already walks configured concept directories and mints ConceptNodes from human-authored markdown.

This change wires up the rationale side: structured frontmatter extraction from ADRs and pattern files, inline `# DECISION:` marker detection in code files, Governs edge emission from markdown frontmatter, and DecisionCard output.

## Goals / Non-Goals

**Goals:**
- Emit `EdgeKind::Governs` from human-authored ADR frontmatter.
- Extract structured rationale fields (title, status, decision text, consequences) from ADR/pattern markdown.
- Extract inline `# DECISION:` markers into `FileNode.inline_decisions`.
- Define DecisionCard as an optional surface output backed by ConceptNodes with Governs edges.
- Define the curated-mode promotion policy for rationale sources.

**Non-Goals:**
- New node types. ConceptNode covers human-authored markdown; no DecisionNode is needed.
- Inferring or synthesizing rationale. All rationale sources are human-authored.
- Making pattern files mandatory. Repos with no docs are unaffected.
- Repair-loop stale-reference detection (belongs in `repair-loop-v1`).
- Overlay commentary on decisions.

## Decisions

### Decision 1: No new node type for rationale

DecisionCard is a view over ConceptNodes that have outgoing Governs edges. Adding a `DecisionNode` would require schema changes, a new ID type, and new storage columns. The existing ConceptNode already covers "human-authored markdown in configured directories." The DecisionCard enriches the surface output without changing the graph schema.

Alternative considered: A `DecisionNode` with a separate ID namespace. Rejected because the trust model requires ConceptNode to be the type boundary for human-authored prose, and a parallel type would duplicate that boundary without adding value.

### Decision 2: Rationale extraction stays in the prose stage (stage 3)

ADR and pattern files are already discovered by stage 3. Structured frontmatter extraction (title, status, governs array) and decision body extraction are additional passes over files already in scope. This avoids duplicating file discovery.

`# DECISION:` markers in code files require a separate scan since code files are not in the prose stage's scope. This scan runs as a sub-pass within stage 3 over non-concept files. It is a single-line text scan, not a tree-sitter parse, so it adds minimal cost.

Alternative considered: Running `# DECISION:` extraction in stage 2 alongside tree-sitter. Rejected because it mixes human-declaration extraction with parser-observation extraction, conflating two epistemic categories in the same stage.

### Decision 3: Governs edge references use file paths, not node IDs

ADR frontmatter `governs:` arrays will reference relative file paths (e.g., `src/store/sqlite/mod.rs`) rather than node IDs. Authors writing ADRs do not know node IDs. Path references are resolved to `FileNodeId` at compile time using the file discovery table.

Tradeoff: Path references become stale after renames. This is accepted for now. The repair-loop (`repair-loop-v1`) will add stale-link detection. At this stage, a stale path reference is silently skipped during edge resolution (no crash; no Governs edge emitted).

### Decision 4: Curated-mode promotion policy is edge-level, not node-level

In both auto and curated mode, ConceptNodes are already created as `ParserObserved` or `HumanDeclared` depending on mode. The new rule is: `Governs` edges emitted from human-authored frontmatter are labeled `HumanDeclared` in both modes. Inline `# DECISION:` markers are stored on the containing `FileNode` and do not create `Governs` edges in this change. The auto versus curated distinction stays on the ConceptNode epistemic label, not on whether the edge exists.

Machine-authored overlay content cannot contribute Governs edges. This is enforced by keeping Governs edge emission exclusively in the prose/rationale pipeline stages, not in the synthesis path.

### Decision 5: DecisionCard is a surface-only type, not a new graph type

DecisionCard fields: decision title (from ConceptNode display name or frontmatter `title`), status (frontmatter `status` if present), decision text (extracted from ADR body), governed node IDs (via outgoing Governs edges), freshness (last git-observed modification date). Budget tiers follow the same `tiny` / `normal` / `deep` pattern as existing card types.

DecisionCard is returned only when a queried node has incoming Governs edges from ConceptNodes. If no rationale links exist, the field is absent from the structural card, not null.

## Risks / Trade-offs

- `src/structure/prose.rs` is the natural home for rationale extraction, but it may approach the 400-line limit if extended significantly. If it does, split into `src/structure/prose/` with `concept.rs` and `rationale.rs` sub-modules before adding the new code.
- Path-based `governs:` references are fragile across renames. Accepted for now; stale detection is `repair-loop-v1` scope.
- `# DECISION:` marker format needs a clear syntax to avoid false positives. Proposal: the marker must be a line-comment at the start of a non-blank line with the exact token `DECISION:` followed by non-empty text (e.g., `// DECISION: use SQLite because...`). Comment syntax varies by language; the scan uses the existing `FileClass` to select the comment prefix.
- DecisionCard adds a new MCP surface output. If the MCP schema is strict about card types, a schema version bump may be needed. Check `crates/synrepo-mcp/` before implementing.

## Open Questions

- What frontmatter keys are canonical for ADRs? The MADR format uses `title`, `status`, `date`, `deciders`. The proposal says to start with `title`, `status`, and `governs`. Defer format normalization to the spec; the extractor should be tolerant of missing fields.

## Decisions

### Decision 6: Inline `# DECISION:` markers are stored on FileNode, not as ConceptNodes

Invariant 7 restricts `ConceptNode` creation to human-authored markdown in configured concept directories. Inline `# DECISION:` markers live in code files, not in concept directories, so they cannot produce `ConceptNode` records.

The resolved approach: store extracted marker text as `inline_decisions: Vec<String>` on the containing `FileNode`. This preserves the decision text without violating the type boundary. Agents can inspect `inline_decisions` on a `FileNode` to see design rationale embedded in code.

Governs edges are NOT emitted for inline markers. The repair-loop (`repair-loop-v1`) may add self-referential or cross-file linking for inline markers in a later phase.

**Alternative considered:** Inline markers produce a transient ConceptNode (not persisted in configured directories). Rejected because it violates invariant 7 and would require special-casing the ConceptNode creation path.
