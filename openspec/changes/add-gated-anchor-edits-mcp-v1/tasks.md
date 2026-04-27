## 1. MCP Gate and Registration

- [ ] Add an explicit `synrepo mcp --allow-edits` flag and wire it into MCP server startup.
- [ ] Keep mutating tools absent from `tools/list` unless `--allow-edits` is present.
- [ ] Document any config interaction as restrictive only, not sufficient to enable edits.
- [ ] Add smoke tests for default read-first tool lists and edit-enabled tool lists.

## 2. Anchor Session Layer

- [ ] Add a small session-scoped anchor manager under `src/surface/mcp/`, using in-memory LRU/TTL unless implementation proves persistence is needed.
- [ ] Key anchor state by repo root, task ID, path, content hash, and opaque anchor state version.
- [ ] Base prepared contexts on existing graph facts where available: file ID, symbol ID, graph epoch, content hash, and source hash.
- [ ] Add unit tests for version changes, stale content hashes, missing sessions, and deterministic anchor ordering.

## 3. `synrepo_prepare_edit_context`

- [ ] Implement the MCP tool in a new surface module rather than growing `src/bin/cli_support/commands/mcp.rs`.
- [ ] Accept path/symbol/range style targets and return compact anchored source context.
- [ ] Return `task_id`, `anchor_state_version`, `path`, `file_id`, `content_hash`, `source_hash`, and prepared anchor lines.
- [ ] Add tests for file target, symbol target, range target, budgeted output, and not-found errors.

## 4. `synrepo_apply_anchor_edits`

- [ ] Implement per-file edit batches with inputs for `task_id`, `anchor_state_version`, `path`, `content_hash`, `anchor`, optional `end_anchor`, `edit_type`, and `text`.
- [ ] Validate anchor session, content hash, anchor existence, end-anchor ordering, and exact current line content before writing.
- [ ] Apply batches atomically per file, returning per-file success or failure for multi-file requests.
- [ ] Reject cross-file atomicity claims in response metadata.
- [ ] Add tests for insert, replace, delete, stale anchor, stale content hash, overlapping edits, ambiguous anchors, and partial multi-file outcomes.

## 5. Writer Lock and Reconcile

- [ ] Acquire existing writer admission before any file mutation.
- [ ] Surface writer-lock conflicts as structured MCP errors with holder information where available.
- [ ] After successful writes, delegate reconcile to the active watch service when authoritative watch is present, otherwise run local reconcile.
- [ ] Add focused tests for writer-lock contention and watch-delegated reconcile behavior.

## 6. Diagnostics

- [ ] Return bounded post-edit diagnostics: validation status, reconcile/delegation status, changed test-surface recommendations, and optional cheap check summary.
- [ ] Do not execute arbitrary commands from MCP edit tools.
- [ ] Add tests proving command execution is unavailable and diagnostics do not mutate beyond existing reconcile/check paths.

## 7. Documentation and Doctrine

- [ ] Update `src/surface/mcp/README.md` with the edit-enabled workflow and default read-first behavior.
- [ ] Update `skill/SKILL.md` and agent-facing guidance to prefer read tools first, then `prepare` before `apply` when edit mode is explicitly enabled.
- [ ] Document that anchors are session-scoped operational state, not canonical graph facts or agent memory.

## 8. Validation

- [ ] Run `cargo fmt --check`.
- [ ] Run focused MCP/bin tests for tool registration and edit workflows.
- [ ] Run focused writer-lock and watch-delegation tests.
- [ ] Run `make ci-lint`.
- [ ] Run focused `cargo test --bin synrepo mcp`.
