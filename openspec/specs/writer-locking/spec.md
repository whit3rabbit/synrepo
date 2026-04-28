# writer-locking spec

## Purpose
Define the shared write-admission and writer-locking contract for runtime
mutations so local CLI, watch, repair, overlay, and MCP edit paths do not
compete or corrupt repository state.

## Requirements

### Requirement: Single-writer runtime mutation

`synrepo` MUST permit at most one active runtime write execution context for a
repository at a time.

#### Scenario: same-thread nested write entry
- **WHEN** a mutating operation already holding writer ownership invokes a
  nested mutating helper on the same thread
- **THEN** the nested helper may reuse the existing write ownership
- **AND** no second ownership record is created

#### Scenario: different-thread same-process acquisition
- **WHEN** one thread holds writer ownership for a repository
- **AND** a different thread in the same process attempts to acquire writer
  ownership for that repository
- **THEN** acquisition MUST fail clearly
- **AND** the second thread MUST NOT mutate runtime state

#### Scenario: foreign live process holds writer ownership
- **WHEN** the writer ownership record points to a live foreign process
- **THEN** acquisition MUST fail with holder information
- **AND** mutation MUST NOT proceed

#### Scenario: stale ownership record
- **WHEN** the writer ownership record points to a terminated process
- **THEN** acquisition MAY replace the stale record
- **AND** the new owner becomes authoritative

### Requirement: Unified write admission

All mutating runtime operations MUST enter through a shared write-admission path.

#### Scenario: watch is authoritative and delegation exists
- **WHEN** a mutating CLI operation targets a repository with an active
  authoritative watch service
- **AND** delegation is supported for that operation
- **THEN** the CLI MUST delegate rather than competing for writer ownership

#### Scenario: watch is authoritative and delegation does not exist
- **WHEN** a mutating CLI operation targets a repository with an active
  authoritative watch service
- **AND** delegation is not supported for that operation
- **THEN** the CLI MUST fail clearly
- **AND** it MUST NOT attempt direct mutation

### Requirement: Atomic multi-store write sequences

Operations that span multiple runtime stores MUST keep their atomicity contract
under shared write admission.

#### Scenario: overlay promotion sequence
- **WHEN** a curated overlay candidate is promoted into graph state
- **THEN** the pending -> graph-write -> promoted sequence MUST execute under
  write ownership
- **AND** crash recovery MUST remain idempotent

### Requirement: Admit MCP source edits through the shared writer path
Edit-enabled MCP source mutation SHALL use the shared write-admission path before modifying source files or runtime state. The edit workflow SHALL respect active watch ownership and existing writer-lock conflict behavior.

#### Scenario: Apply edit while no writer is active
- **WHEN** `synrepo_apply_anchor_edits` validates an edit batch
- **AND** no foreign writer owns the repository
- **THEN** synrepo acquires writer admission before writing files
- **AND** releases writer admission after the write and post-edit runtime update attempt complete

#### Scenario: Apply edit while another writer is active
- **WHEN** `synrepo_apply_anchor_edits` attempts to write
- **AND** a live foreign process holds writer ownership
- **THEN** synrepo rejects the mutation with structured holder information when available
- **AND** no source file is modified

#### Scenario: Apply edit while watch is authoritative
- **WHEN** `synrepo_apply_anchor_edits` writes a file while an authoritative watch service owns the repository
- **THEN** synrepo uses the existing watch delegation path for reconcile when available
- **AND** it does not start a second independent graph mutation path

#### Scenario: Reconcile is unavailable after write
- **WHEN** a source edit succeeds
- **AND** reconcile cannot be completed or delegated
- **THEN** the edit response marks runtime graph freshness as stale or unknown
- **AND** the response recommends the existing operator action to reconcile
