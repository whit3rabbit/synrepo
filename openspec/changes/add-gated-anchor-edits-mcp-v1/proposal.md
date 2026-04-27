## Why

The MCP surface is intentionally read-first today, which is the right default. It helps agents find context, cards, risks, and test surfaces without giving them write authority. The next useful Dirac idea to port is not command execution. It is validated anchored source edits: small edit batches that name a prepared source range, prove freshness with hashes and versions, and let synrepo validate the current file before writing.

Without a dedicated mutation gate and edit protocol, agents either emit broad patches outside synrepo or ask for raw source context and rely on brittle line numbers. That loses the advantage of synrepo's graph facts, content hashes, source hashes, and writer-lock discipline.

## What Changes

- Add an explicit process-level mutating mode gate for MCP:
  - Default `synrepo mcp` remains read-first.
  - Mutating tools appear only when the server is started with an obvious invocation such as `synrepo mcp --allow-edits`.
  - Configuration may further restrict editing, but it must not be the only way to discover that editing is enabled.
- Add two mutating-mode MCP tools:
  - `synrepo_prepare_edit_context`
  - `synrepo_apply_anchor_edits`
- Add a session-scoped operational anchor layer based on synrepo graph facts:
  - Anchors are prepared from current file IDs, symbol IDs, graph epoch, content hash, and source hash data where available.
  - Line anchors are not canonical graph state and are not agent memory.
  - Initial storage is in-memory with LRU/TTL unless implementation discovers a concrete need for `.synrepo/state/edit-sessions/`.
- Apply edit batches atomically per file first:
  - Validate `task_id`, `anchor_state_version`, `path`, `content_hash`, `anchor`, optional `end_anchor`, `edit_type`, and `text`.
  - Validate anchors against current file content before writing.
  - Acquire the existing writer lock before mutation.
  - Write each file atomically.
  - Return per-file success or failure for multi-file batches without claiming cross-file atomicity.
- After successful writes, run cheap built-in diagnostics:
  - Trigger reconcile locally or delegate to the active watch service.
  - Return changed test-surface recommendations and operational diagnostics.
  - Keep arbitrary command execution out of v1.

## Impact

- Affected specs:
  - `mcp-surface`
  - `anchored-edits` (new capability)
  - `writer-locking`
- Affected code areas:
  - MCP CLI argument parsing and server initialization.
  - MCP tool registration in `src/bin/cli_support/commands/mcp.rs`, while keeping heavy logic in `src/surface/mcp/`.
  - New anchored-edit surface modules under `src/surface/mcp/`.
  - Writer admission and watch/reconcile integration.
  - Tests for tool visibility, validation failures, per-file atomicity, stale anchors, writer-lock contention, and post-edit diagnostics.

## Non-Goals

- No MCP command execution tool.
- No arbitrary shell, lint, or test execution in v1.
- No cross-file transaction guarantee.
- No canonical graph mutation for line anchors.
- No persistent agent memory or caller-history store.
- No hidden config-only edit enablement.

## Open Questions

- Should v1 persist edit sessions under `.synrepo/state/edit-sessions/`, or is an in-memory LRU/TTL sufficient for the first implementation?
- What exact anchor token format should be used: human-readable words, stable short IDs, or a compact generated form? The protocol should not depend on the visual form.
- Should post-edit `synrepo check` run by default after every successful file write, or only under a caller-specified diagnostics budget?
