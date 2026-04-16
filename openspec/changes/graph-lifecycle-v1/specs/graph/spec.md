## Delta: graph-lifecycle-v1

### Requirement: Stabilize file identity across content edits
synrepo SHALL preserve `FileNodeId` across in-place content edits. A content-hash change SHALL advance the `content_hash` version field on `FileNode` without triggering node deletion. Physical deletion of a file node SHALL be reserved for files genuinely absent from the repository after the identity cascade and for the compaction maintenance pass.

#### Scenario: Edit a file in place and re-compile
- **WHEN** a user edits a file's content and the structural compile runs
- **THEN** the `FileNode` retains its original `FileNodeId`
- **AND** the `content_hash` field is updated to reflect the new content
- **AND** no symbols or edges are cascade-deleted as a side effect of the content change

#### Scenario: Delete a file from the repository
- **WHEN** a file is removed from the repository and the identity cascade finds no rename, split, or merge match
- **THEN** the file node and all its owned observations are physically deleted
- **AND** drift scoring records the deletion as endpoint score 1.0

### Requirement: Carry observation ownership on parser-emitted facts
synrepo SHALL record `owner_file_id` on every parser-emitted symbol and edge, identifying the file whose parse pass produced the observation. Recompiling a file SHALL retire only observations owned by that file, leaving observations owned by other files untouched.

#### Scenario: Recompile one file in a multi-file graph
- **WHEN** the structural compile reprocesses file A while file B is unchanged
- **THEN** only observations with `owner_file_id = A` are subject to retirement
- **AND** observations owned by file B retain their `last_observed_rev` and `retired_at_rev` state

### Requirement: Soft-retire non-observed facts instead of deleting them
synrepo SHALL mark parser-observed symbols and edges as retired (`retired_at_rev = current_revision`) when they are not re-emitted during a structural compile pass. Retired facts SHALL remain physically present until removed by the compaction maintenance pass. Re-emission of a previously retired fact SHALL clear `retired_at_rev` and advance `last_observed_rev`.

#### Scenario: A symbol is removed from a file
- **WHEN** a file edit removes a previously observed symbol and the structural compile runs
- **THEN** the symbol's `retired_at_rev` is set to the current compile revision
- **AND** the symbol remains queryable via `all_edges()` for drift scoring
- **AND** `active_edges()` and `symbols_for_file()` exclude the retired symbol

#### Scenario: A previously retired symbol reappears
- **WHEN** a file edit re-introduces a symbol that was retired in a prior compile revision
- **THEN** the symbol's `retired_at_rev` is cleared
- **AND** `last_observed_rev` is set to the current compile revision

### Requirement: Respect epistemic ownership boundaries during retirement
synrepo SHALL NOT retire human-declared facts (`Epistemic::HumanDeclared`) during a parser compile pass. Only the emitter class that produced a fact (parser, git, human) may retire it. Parser passes SHALL retire only `ParserObserved` observations.

#### Scenario: A parser pass encounters a human-declared Governs edge
- **WHEN** the structural compile refreshes a file that has an incident `Governs` edge with `Epistemic::HumanDeclared`
- **THEN** the `Governs` edge is not retired regardless of whether it was re-emitted by the parser
- **AND** only `ParserObserved` edges owned by the file are subject to retirement

### Requirement: Track compile revisions as a monotonic counter
synrepo SHALL maintain a `compile_revisions` table with a monotonically increasing integer revision counter. Each structural compile SHALL increment the counter. All observation-window fields (`last_observed_rev`, `retired_at_rev`) SHALL reference this counter.

#### Scenario: Two compiles within the same git commit
- **WHEN** a user runs `synrepo init` twice without any git commits between runs
- **THEN** each compile increments the revision counter independently
- **AND** observation windows distinguish the two compiles by their revision numbers

### Requirement: Compact retired observations as a maintenance operation
synrepo SHALL provide a compaction pass that physically deletes retired symbols, edges, and associated sidecar data older than a configurable retention window (`retain_retired_revisions`, default 10). Compaction SHALL run during `synrepo sync` and `synrepo upgrade --apply`, never during the hot reconcile path.

#### Scenario: Compact after the retention window expires
- **WHEN** `synrepo sync` runs and retired observations exist with `retired_at_rev < current_rev - retain_retired_revisions`
- **THEN** those symbols and edges are physically deleted from the store
- **AND** associated `edge_drift` and `file_fingerprints` rows older than the retention window are also removed
- **AND** active (non-retired) observations are never deleted by compaction
