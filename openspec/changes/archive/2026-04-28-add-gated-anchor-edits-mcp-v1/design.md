## Context

Dirac's strongest non-read-only idea is the edit protocol, not its command runner. The useful part for synrepo is a small, validated edit surface where the model identifies a prepared source range and emits only replacement text. Synrepo already has graph epochs, file IDs, symbol IDs, content hashes, source hashes, watch delegation, and writer locks. Those should form the trust and freshness base.

The design must preserve the current product boundary: MCP is read-first by default, and write capability must be visible in the process invocation.

## Decisions

### Explicit Process Gate

`synrepo mcp` keeps the default read-first tool list. `synrepo mcp --allow-edits` enables edit tools. A config value may disable or further constrain edits, but config alone must not enable them.

This makes the risk visible to a user reading a process list, terminal history, agent setup, or MCP config. It also lets existing clients keep their current safety posture with no behavior change.

### Two-Tool Protocol

`synrepo_prepare_edit_context` prepares an anchored view for a path, symbol, range, or context-pack-derived target. It returns:

- `task_id`
- `anchor_state_version`
- `repo_root`
- `graph_epoch`
- `path`
- `file_id`
- `content_hash`
- `source_hash`
- prepared anchors and boundary line text
- compact source context sized by budget

`synrepo_apply_anchor_edits` applies one or more anchored edits. Inputs include:

- `task_id`
- `anchor_state_version`
- `path`
- `content_hash`
- `anchor`
- optional `end_anchor`
- `edit_type`
- `text`

The apply tool validates freshness and line content against the current file before writing. It rejects stale or ambiguous anchors with a structured conflict response and enough refreshed context for the caller to retry.

### Session-Scoped Anchors

Line anchors are operational session state. They are not graph nodes, overlay content, commentary, or agent memory. The initial implementation should prefer an in-memory LRU/TTL keyed by `{repo_root, task_id, path, anchor_state_version}`. Persistent `.synrepo/state/edit-sessions/` storage is reserved for a later requirement if short-lived MCP restarts make it necessary.

Anchor versions are opaque. They should advance whenever the prepared anchor set is reconciled or invalidated. They are not a replacement for `content_hash`; callers must provide both.

### Per-File Atomicity First

Within one file, a batch is validated against a single current snapshot and written atomically. Multi-file calls are allowed for ergonomics, but the result is a list of per-file outcomes. The server must not report a multi-file batch as one transaction.

This avoids promising rollback semantics before synrepo has backup snapshots or a workspace transaction model.

### Writer Admission and Reconcile

Applying source edits is a local mutation and must pass through existing writer admission before writing. After a successful write, synrepo must refresh runtime truth by either:

- requesting reconcile through the active watch service when watch is authoritative and delegation is available, or
- running the local reconcile path when no watch service is active.

The edit tool should not directly mutate graph facts to match its edits. The graph remains produced by the structural pipeline.

### Diagnostics Without Command Execution

V1 diagnostics are built-in and bounded:

- anchor validation result
- content-hash freshness result
- reconcile or watch-delegation result
- cheap `synrepo check` style drift summary when requested by diagnostics budget
- changed test-surface recommendations from existing synrepo surfaces

Arbitrary shell command execution is out of scope. Configured lint/test command execution can be proposed later with a separate security design.

## Risks and Mitigations

- **Hidden write authority:** Require `--allow-edits` in the process invocation and test that tools are absent by default.
- **Stale anchors:** Validate both `anchor_state_version` and `content_hash`, then verify current line text before writing.
- **Concurrent writes:** Use existing writer admission and fail clearly on foreign holders.
- **Watch conflicts:** Reuse watch delegation semantics instead of creating a second reconcile path.
- **Overpromised atomicity:** Guarantee only per-file atomicity in v1 and expose per-file statuses for multi-file batches.
- **Graph corruption:** Do not write graph facts directly from the edit tool. Reconcile remains the graph producer.

## Alternatives Considered

- **Config-only edit enablement:** Rejected because the running process would not make write authority obvious.
- **Command execution first:** Rejected because shell access is a larger security surface than validated source edits.
- **Canonical graph anchors:** Rejected because line anchors are volatile operational state, while graph facts should remain structural observations.
- **Cross-file transaction:** Deferred because it requires rollback snapshots or a broader workspace transaction contract.
