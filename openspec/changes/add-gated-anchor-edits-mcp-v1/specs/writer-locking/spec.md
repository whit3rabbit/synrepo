## ADDED Requirements

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
