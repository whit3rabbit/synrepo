## ADDED Requirements

### Requirement: Build A Repo-Scoped Resume Packet
Synrepo SHALL build an explicit repo-scoped resume packet from existing repository state and advisory surfaces. The packet SHALL include `schema_version`, `packet_type`, `repo_root`, `generated_at`, `context_state`, `sections`, `detail_pointers`, and `omitted`.

#### Scenario: Packet includes bounded continuation sections
- **WHEN** a caller requests repo resume context
- **THEN** synrepo returns sections for changed files, next actions, recent activity, saved notes, and validation guidance when those sources are available
- **AND** the packet identifies the source store for derived sections

#### Scenario: Empty repository state still returns a useful packet
- **WHEN** no changed files, handoffs, recent activity, or notes are available
- **THEN** synrepo returns an empty but valid packet with validation guidance and detail pointers

### Requirement: Keep Resume Context Out Of Generic Session Memory
The resume packet SHALL NOT store or expose prompts, chat history, raw tool outputs, caller identity, or automatic hook-captured events. It SHALL be derived from current repo state, synrepo operational state, explicit overlay notes, and aggregate metrics only.

#### Scenario: Caller requests resume context after a long session
- **WHEN** synrepo builds the packet
- **THEN** the packet does not include prompt text, assistant responses, raw command output, or session transcript data
- **AND** no new saved-context record is written

### Requirement: Summarize Advisory Notes Only
When notes are included, synrepo SHALL return bounded note summaries only: note id, target, lifecycle status, confidence, updated time, and a short claim preview. Full note detail SHALL remain available through explicit note surfaces.

#### Scenario: Notes exist in the overlay store
- **WHEN** the caller requests resume context with notes enabled
- **THEN** the packet includes note summaries labeled `source_store: "overlay"` and `advisory: true`
- **AND** omitted or hidden note detail can be fetched through a listed pointer

### Requirement: Trim Lower-Priority Sections First
Synrepo SHALL enforce the requested resume packet token cap by preserving critical continuation state before lower-priority advisory sections. The response SHALL remain valid JSON and SHALL report omitted sections or items.

#### Scenario: Packet exceeds token budget
- **WHEN** the assembled packet is larger than the effective token cap
- **THEN** synrepo removes lower-priority sections or items before removing changed files, validation guidance, or detail pointers
- **AND** `context_state.truncation_applied` is true
- **AND** `omitted` explains what was dropped
