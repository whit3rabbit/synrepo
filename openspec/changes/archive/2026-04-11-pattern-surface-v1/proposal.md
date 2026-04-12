## Why

Milestone 3 shipped the five-tool MCP surface (`synrepo_overview`, `synrepo_card`, `synrepo_search`, `synrepo_where_to_edit`, `synrepo_change_impact`), plus `SymbolCard`, `FileCard`, and file-facing git intelligence. That gives agents deterministic orientation on code structure alone, but it does not yet surface design intent. Milestone 4 adds the first human-guidance layer: pattern documents, ADR ingestion, inline rationale markers, and DecisionCards. Without this, teams with existing architectural docs have no way to surface design intent alongside structural facts, and agents answer "what is here" but not "why it was built this way."

## What Changes

- Define the pattern document format and allowed directory locations.
- Define rationale extraction rules for ADRs and inline `# DECISION:` markers.
- Define linking rules connecting pattern/decision sources to `FileNode` and `ConceptNode` graph entries without violating the markdown-only `ConceptNode` invariant.
- Introduce `DecisionCard` as an optional surface output backed by human-authored rationale and linked to structural cards.
- Define curated-mode promotion rules: conditions under which rationale sources become `HumanDeclared` graph entries via `EdgeKind::Governs`.
- Keep structural cards primary; repos with no docs continue to work without change.

## Capabilities

### New Capabilities

_(none — both affected specs already exist; this change expands their requirements)_

### Modified Capabilities

- `patterns-and-rationale`: Expand from stub (three scenarios) to full spec covering pattern file format, allowed locations, ADR ingestion rules, inline `# DECISION:` marker extraction, linking rules between rationale sources and graph nodes, and curated promotion policy.
- `cards`: Add `DecisionCard` contract covering required fields, budget tier behavior, freshness labeling, and the rule that DecisionCard content never overrides structural card truth.

## Impact

- `src/structure/prose.rs` — extend concept extractor or add a parallel rationale extractor for ADRs and inline markers.
- `src/structure/graph/` — `EdgeKind::Governs` is already defined; ensure it is emitted from human-authored frontmatter and `# DECISION:` markers per the new promotion rules.
- `src/surface/card/` — add `DecisionCard` type alongside the shipped structural card types.
- `crates/synrepo-mcp/src/main.rs` — extend `synrepo_card` responses to attach `DecisionCard` when rationale exists.
- `openspec/specs/patterns-and-rationale/spec.md` — replace stub with full requirements.
- `openspec/specs/cards/spec.md` — add `DecisionCard` requirement.
- No new dependencies expected; rationale parsing fits within tree-sitter and existing markdown extraction paths.
