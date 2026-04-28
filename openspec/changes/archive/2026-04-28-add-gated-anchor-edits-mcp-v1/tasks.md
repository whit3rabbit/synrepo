## 1. MCP Gate and Registration

- [x] Add an explicit `synrepo mcp --allow-edits` flag and wire it into MCP server startup.
- [x] Keep mutating tools absent from `tools/list` unless `--allow-edits` is present.
- [x] Document any config interaction as restrictive only, not sufficient to enable edits.
- [x] Add smoke tests for default read-first tool lists and edit-enabled tool lists.

## 2. Anchor Session Layer

- [x] Add a small session-scoped anchor manager under `src/surface/mcp/`, using in-memory LRU/TTL unless implementation proves persistence is needed.
- [x] Key anchor state by repo root, task ID, path, content hash, and opaque anchor state version.
- [x] Base prepared contexts on existing graph facts where available: file ID, symbol ID, graph epoch, content hash, and source hash.
- [x] Add unit tests for version changes, stale content hashes, missing sessions, and deterministic anchor ordering.

## 3. `synrepo_prepare_edit_context`

- [x] Implement the MCP tool in a new surface module rather than growing `src/bin/cli_support/commands/mcp.rs`.
- [x] Accept path/symbol/range style targets and return compact anchored source context.
- [x] Return `task_id`, `anchor_state_version`, `path`, `file_id`, `content_hash`, `source_hash`, and prepared anchor lines.
- [x] Add tests for file target, symbol target, range target, budgeted output, and not-found errors.

## 4. `synrepo_apply_anchor_edits`

- [x] Implement per-file edit batches with inputs for `task_id`, `anchor_state_version`, `path`, `content_hash`, `anchor`, optional `end_anchor`, `edit_type`, and `text`.
- [x] Validate anchor session, content hash, anchor existence, end-anchor ordering, and exact current line content before writing.
- [x] Apply batches atomically per file, returning per-file success or failure for multi-file requests.
- [x] Reject cross-file atomicity claims in response metadata.
- [x] Add tests for insert, replace, delete, stale anchor, stale content hash, overlapping edits, ambiguous anchors, and partial multi-file outcomes.

## 5. Writer Lock and Reconcile

- [x] Acquire existing writer admission before any file mutation.
- [x] Surface writer-lock conflicts as structured MCP errors with holder information where available.
- [x] After successful writes, delegate reconcile to the active watch service when authoritative watch is present, otherwise run local reconcile.
- [x] Add focused tests for writer-lock contention and watch-delegated reconcile behavior.

## 6. Diagnostics

- [x] Return bounded post-edit diagnostics: validation status, reconcile/delegation status, changed test-surface recommendations, and optional cheap check summary.
- [x] Do not execute arbitrary commands from MCP edit tools.
- [x] Add tests proving command execution is unavailable and diagnostics do not mutate beyond existing reconcile/check paths.

## 7. Documentation and Doctrine

- [x] Update `src/surface/mcp/README.md` with the edit-enabled workflow and default read-first behavior.
- [x] Update `skill/SKILL.md` and agent-facing guidance to prefer read tools first, then `prepare` before `apply` when edit mode is explicitly enabled.
- [x] Document that anchors are session-scoped operational state, not canonical graph facts or agent memory.

## 8. Validation

- [x] Run `cargo fmt --check`.
- [x] Run focused MCP/bin tests for tool registration and edit workflows.
- [x] Run focused writer-lock and watch-delegation tests.
- [x] Run `make ci-lint`.
- [x] Run focused `cargo test --bin synrepo mcp`.
