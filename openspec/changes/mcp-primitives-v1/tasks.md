## 1. MCP handler split

- [ ] 1.1 Create `crates/synrepo-mcp/src/tools/` directory with `mod.rs` that re-exports handler groups
- [ ] 1.2 Move card-family handlers (synrepo_card, synrepo_entrypoints, synrepo_module_card, synrepo_public_api, synrepo_minimum_context) and their param structs into `tools/cards.rs`
- [ ] 1.3 Move search-family handlers (synrepo_search, synrepo_overview, synrepo_where_to_edit, synrepo_change_impact) and their param structs into `tools/search.rs`
- [ ] 1.4 Move audit-family handlers (synrepo_findings, synrepo_recent_activity) and their param structs into `tools/audit.rs`
- [ ] 1.5 Verify `cargo build --workspace` and `cargo test --workspace` pass after the split

## 2. Primitives implementation

- [ ] 2.1 Create `crates/synrepo-mcp/src/tools/primitives.rs` with param structs for all five primitives
- [ ] 2.2 Implement `synrepo_node` handler: parse NodeId from string, dispatch to get_file/get_symbol/get_concept, serialize to JSON, handle invalid/not-found errors
- [ ] 2.3 Implement `synrepo_edges` handler: parse NodeId and direction, call outbound/inbound with optional EdgeKind filter, serialize edges with provenance
- [ ] 2.4 Implement `synrepo_query` handler: inline the CLI query parser (parse direction, node ID, optional edge kind), execute via GraphStore, serialize results
- [ ] 2.5 Implement `synrepo_overlay` handler: parse NodeId, verify node exists in graph, call commentary_for and links_for on overlay store, serialize with null sentinel
- [ ] 2.6 Implement `synrepo_provenance` handler: parse NodeId, retrieve node provenance, retrieve all inbound+outbound edges, serialize node provenance plus per-edge provenance with peer node IDs

## 3. Shared helpers

- [ ] 3.1 Move `with_graph_snapshot`, `parse_budget`, `render_result`, `lift_commentary_text`, `attach_decision_cards` into `tools/helpers.rs` (or keep in a shared location accessible to all tool files)
- [ ] 3.2 Ensure `SynrepoState` and `SynrepoServer` remain in main.rs, with tool modules accessing state via the handler pattern

## 4. Registration

- [ ] 4.1 Register all five primitive handlers in the `#[tool_router]` on `SynrepoServer`
- [ ] 4.2 Add all five tools to the `capabilities` list in `get_info()`

## 5. Verification

- [ ] 5.1 `cargo build --workspace` passes
- [ ] 5.2 `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] 5.3 `cargo test --workspace` passes (existing tests unchanged)
- [ ] 5.4 Manual smoke test: start MCP server, call each primitive via JSON-RPC, verify correct responses for valid and invalid inputs
- [ ] 5.5 `openspec validate` passes
