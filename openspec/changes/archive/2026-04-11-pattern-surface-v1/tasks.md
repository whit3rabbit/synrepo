## 1. Durable spec updates

- [x] 1.1 Replace stub in `openspec/specs/patterns-and-rationale/spec.md` with full requirements from `changes/pattern-surface-v1/specs/patterns-and-rationale/spec.md`
- [x] 1.2 Add DecisionCard requirement to `openspec/specs/cards/spec.md` from `changes/pattern-surface-v1/specs/cards/spec.md`

## 2. ConceptNode schema extension

- [x] 2.1 Add `status: Option<String>` and `decision_body: Option<String>` to `ConceptNode` in `src/structure/graph/node.rs`
- [x] 2.2 Update SQLite store in `src/store/sqlite/` to serialize and deserialize the two new `ConceptNode` fields (migration: treat missing columns as `NULL`, no rebuild required)
- [x] 2.3 Update `prose::extract_concept` in `src/structure/prose.rs` to populate `status` from frontmatter `status:` key and `decision_body` from content after the first `## Decision` heading (or `## Context` if no Decision heading exists)

## 3. Governs edge emission from ADR frontmatter

- [x] 3.1 Add `extract_governs_paths(frontmatter: &str) -> Vec<String>` helper in `src/structure/prose.rs` using the existing `extract_frontmatter_list` pattern
- [x] 3.2 Return governs paths alongside the `ConceptNode` from `extract_concept` (extend return type or add a parallel extraction call at the call site in the pipeline)
- [x] 3.3 In `src/pipeline/structural/mod.rs` stage 3 loop: for each emitted `ConceptNode`, resolve each governs path to a `FileNodeId` via the current file map, and upsert an `EdgeKind::Governs` edge labeled `Epistemic::HumanDeclared`
- [x] 3.4 Stale-reference handling: if a governs path does not resolve to a known `FileNodeId`, skip it silently (no error, no edge)
- [x] 3.5 Add unit test in `src/structure/prose.rs` verifying `extract_governs_paths` parses YAML list format correctly (including empty list and missing key cases)
- [x] 3.6 Add integration test: after `synrepo init` on a fixture repo with an ADR containing `governs: [src/lib.rs]`, `graph query inbound <lib_file_id> governs` returns the concept node

## 4. Inline `# DECISION:` marker support

- [x] 4.1 Resolve open question from `design.md`: inline markers cannot create `ConceptNode` records (invariant 7). Store extracted markers as `inline_decisions: Vec<String>` on `FileNode`. Document this decision in `design.md` under "Decisions."
- [x] 4.2 Add `inline_decisions: Vec<String>` field to `FileNode` in `src/structure/graph/node.rs` and update the SQLite store accordingly (nullable JSON column)
- [x] 4.3 Add `extract_inline_decisions(content: &[u8], file_class: &FileClass) -> Vec<String>` in `src/structure/rationale.rs` (new file): scans for lines where the language-appropriate comment prefix is followed by `DECISION:` and non-empty text, returns the text after `DECISION:`
- [x] 4.4 In stage 2 of `src/pipeline/structural/mod.rs` (or stage 3 secondary scan): call `extract_inline_decisions` for each non-concept code file and store results on the `FileNode`
- [x] 4.5 Add unit tests in `src/structure/rationale.rs` for Rust (`//`), Python (`#`), and TypeScript (`//`) comment prefixes, including false-positive guard (e.g., `// DECISIONS:` should not match)

## 5. DecisionCard

- [x] 5.1 Add `src/surface/card/decision.rs` defining `DecisionCard` struct: `title: String`, `status: Option<String>`, `decision_body: Option<String>`, `governed_node_ids: Vec<NodeId>`, `source_path: String`, `freshness: Freshness`
- [x] 5.2 Implement budget tier rendering on `DecisionCard`: `tiny` emits `title` and `governed_node_ids` only; `normal` adds `status` and `decision_body` truncated to 300 chars; `deep` emits all fields
- [x] 5.3 Export `DecisionCard` from `src/surface/card/mod.rs`
- [x] 5.4 Add `find_governing_concepts(node_id: &NodeId) -> Result<Vec<ConceptNode>>` query to `GraphStore` trait and `SqliteGraphStore` implementation: returns `ConceptNode`s that have an outgoing `Governs` edge to the given node

## 6. MCP surface integration

- [x] 6.1 In `crates/synrepo-mcp/`: extend `synrepo_card` to call `find_governing_concepts`; if results are non-empty, build and attach `DecisionCard` data to the response
- [x] 6.2 Verify the MCP JSON schema accepts an optional `decision_card` field in tool responses without breaking callers that do not expect it (absent field, not null)

## 7. Tests and validation

- [x] 7.1 Add snapshot test (insta) for `DecisionCard` rendered at `tiny`, `normal`, and `deep` tiers
- [x] 7.2 Run `make check` (fmt-check + clippy + tests) with zero warnings
- [x] 7.3 Manually verify with `RUST_LOG=debug cargo run -- init` on a fixture with an ADR that the Governs edges appear in `cargo run -- graph stats` output
