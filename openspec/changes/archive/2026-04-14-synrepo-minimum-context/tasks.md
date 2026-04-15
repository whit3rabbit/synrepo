## 1. Neighborhood resolution module

- [x] 1.1 Create `src/surface/card/compiler/neighborhood.rs` with the `MinimumContextResponse` struct (focal card JSON, neighbors list, co-change partners, edge counts) and the `resolve_neighborhood` function signature
- [x] 1.2 Implement focal node resolution: accept node ID string or qualified path via existing `resolve_target`, return explicit error for unresolved targets
- [x] 1.3 Implement structural neighbor resolution at `normal` budget: query `outbound(from, Some(Calls))` and `outbound(from, Some(Imports))`, cap at 10 per kind, return summaries (node ID, qualified name, kind, edge type)
- [x] 1.4 Implement structural neighbor resolution at `deep` budget: produce full cards for outbound neighbors instead of summaries
- [x] 1.5 Implement `tiny` budget path: focal card only, with edge counts (`outbound_calls_count`, `outbound_imports_count`, `governs_count`, `co_change_count`) and no neighbor details
- [x] 1.6 Implement governing decisions: call `find_governing_concepts(node_id)`, produce DecisionCard summaries at `normal`, full DecisionCards at `deep`
- [x] 1.7 Implement co-change partner resolution: read from git-intelligence cache via `resolve_file_git_intelligence`, rank by co-change count, cap at top-3 (normal) or top-5 (deep), label each entry with `source: "git_intelligence"` and `granularity: "file"`
- [x] 1.8 Handle missing co-change data: return empty list with `co_change_state: "missing"` when git intelligence is unavailable or degraded

## 2. Read snapshot enforcement

- [x] 2.1 Wrap the entire `resolve_neighborhood` call in `with_graph_read_snapshot` to ensure a consistent epoch across focal card, structural neighbors, and governing decisions
- [x] 2.2 Verify git-intelligence cache reads do not require their own snapshot (cache is session-scoped and in-memory, not graph-backed)

## 3. MCP tool wiring

- [x] 3.1 Add `MinimumContextParams` struct in `crates/synrepo-mcp/src/main.rs` with `target: String` (required) and `budget: Option<String>` (default `"normal"`)
- [x] 3.2 Add `synrepo_minimum_context` tool handler: resolve target, call `resolve_neighborhood`, serialize response as JSON, handle errors (target not found, budget parse failure)
- [x] 3.3 Ensure the tool does not include overlay content: omit `overlay_commentary`, `proposed_links`, `commentary_state`, and `links_state` from the focal card in the response

## 4. Tests

- [x] 4.1 Unit test: `resolve_neighborhood` at `tiny` budget returns focal card with edge counts and no neighbor details
- [x] 4.2 Unit test: `resolve_neighborhood` at `normal` budget returns summaries for outbound structural neighbors and top-3 co-change partners
- [x] 4.3 Unit test: `resolve_neighborhood` at `deep` budget returns full cards for neighbors and top-5 co-change partners
- [x] 4.4 Unit test: unresolved target returns explicit error with the target string
- [x] 4.5 Unit test: missing git-intelligence data returns empty co-change list with `co_change_state: "missing"`
- [x] 4.6 Unit test: governing decisions are included as DecisionCard summaries at `normal` and full DecisionCards at `deep`
- [x] 4.7 Snapshot test for the full `MinimumContextResponse` at each budget tier

## 5. Validation

- [x] 5.1 Run `cargo test` and confirm all tests pass
- [x] 5.2 Run `cargo clippy --workspace --all-targets -- -D warnings` and confirm no new warnings
- [x] 5.3 Run `make check` for full CI-equivalent validation
- [x] 5.4 Smoke test: `synrepo-mcp` startup and tool list includes `synrepo_minimum_context`
