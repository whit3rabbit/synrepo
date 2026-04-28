# anchored-edits Specification

## Purpose
Define the validated anchored-edit protocol used by edit-enabled MCP clients.
Anchors are short-lived operational state for source edits, not canonical graph
facts, overlay content, commentary, or agent memory.

## Requirements
### Requirement: Prepare session-scoped source anchors from current graph facts
synrepo SHALL prepare line anchors as session-scoped operational state for edit workflows. Prepared anchor state SHALL include an opaque `anchor_state_version` and SHALL be based on current repository facts where available, including `graph_epoch`, file ID, symbol ID, content hash, and source hash. Prepared line anchors SHALL NOT be stored as canonical graph facts, overlay content, commentary, or agent memory.

#### Scenario: Prepare anchors for a file target
- **WHEN** an edit-enabled MCP client requests edit context for a file path
- **THEN** synrepo returns compact source context with line anchors
- **AND** the response includes `task_id`, `anchor_state_version`, `path`, `file_id`, `content_hash`, `source_hash`, and `graph_epoch` when those values are available

#### Scenario: Prepared anchors expire or are evicted
- **WHEN** an anchor session has expired or been evicted from the operational cache
- **AND** a client attempts to apply edits using that session
- **THEN** synrepo rejects the request as unprepared or expired
- **AND** no source file is modified

#### Scenario: Anchors are not persisted as graph facts
- **WHEN** a reconcile pass runs after anchors were prepared
- **THEN** the graph contains structural observations only
- **AND** prepared line anchors do not appear as graph nodes, graph edges, commentary, or agent notes

### Requirement: Validate anchored edits against current file content before writing
synrepo SHALL validate every anchored edit against the current file content before writing. Validation SHALL require the caller's `content_hash` to match the current file content, the `anchor_state_version` to identify a live prepared state, the start anchor to exist, any end anchor to exist in the same file and follow the start anchor, and the anchored boundary line text to match the current file exactly.

#### Scenario: Valid replacement edit writes the file
- **WHEN** a client applies a replacement edit with a live anchor state
- **AND** the supplied `content_hash` matches the current file
- **AND** the start and end anchor boundary text matches the current file exactly
- **THEN** synrepo writes the replacement atomically for that file
- **AND** returns the new content hash or refreshed state metadata

#### Scenario: Stale content hash rejects the edit
- **WHEN** a file changed after edit context was prepared
- **AND** a client applies an edit with the old `content_hash`
- **THEN** synrepo rejects the edit as stale
- **AND** no source file is modified

#### Scenario: Boundary text mismatch rejects the edit
- **WHEN** an anchor name still exists in the session
- **BUT** the current file line no longer matches the prepared boundary line text
- **THEN** synrepo rejects the edit as stale or conflicted
- **AND** no source file is modified

### Requirement: Apply edit batches atomically per file
synrepo SHALL treat one file as the first atomicity boundary for anchored edit batches. A batch containing multiple edits for one file SHALL validate against one current file snapshot and either write that file once or not at all. A batch containing multiple files MAY return mixed per-file outcomes and SHALL NOT claim cross-file transaction semantics.

#### Scenario: One file batch succeeds as one write
- **WHEN** a batch contains multiple non-overlapping edits for one file
- **AND** all edits validate against the same current file snapshot
- **THEN** synrepo applies the edits and writes the file atomically once
- **AND** the response reports that file as applied

#### Scenario: One invalid edit cancels that file write
- **WHEN** a batch contains multiple edits for one file
- **AND** any edit in that file fails validation
- **THEN** no edits are written to that file
- **AND** the response reports the validation failure

#### Scenario: Multi-file batch returns per-file outcomes
- **WHEN** a batch contains edits for two files
- **AND** the first file validates but the second file fails validation
- **THEN** synrepo may apply the first file and reject the second file
- **AND** the response reports separate per-file success and failure
- **AND** the response does not describe the batch as cross-file atomic

### Requirement: Return bounded post-edit diagnostics
After a successful anchored edit write, synrepo SHALL return bounded diagnostics covering validation status, write status, reconcile or watch-delegation status, and changed test-surface recommendations. The diagnostics SHALL NOT require arbitrary command execution.

#### Scenario: Successful edit returns reconcile status
- **WHEN** an anchored edit writes a file successfully
- **THEN** synrepo triggers the configured reconcile path or delegates reconcile to the active watch service
- **AND** the response includes whether reconcile was completed, delegated, or failed

#### Scenario: Successful edit returns test-surface recommendations
- **WHEN** an anchored edit writes a file successfully
- **THEN** synrepo returns changed test-surface recommendations derived from existing graph or card surfaces when available
- **AND** the response labels absent recommendations distinctly from a diagnostics failure

#### Scenario: Diagnostics avoid arbitrary commands
- **WHEN** post-edit diagnostics run
- **THEN** synrepo does not execute caller-provided shell commands
- **AND** diagnostics are limited to built-in checks and existing synrepo surfaces
