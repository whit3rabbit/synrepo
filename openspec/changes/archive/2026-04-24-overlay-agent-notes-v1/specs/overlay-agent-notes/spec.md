## ADDED Requirements

### Requirement: Define agent notes as overlay-only advisory content
synrepo SHALL define agent notes as advisory overlay records attached to explicit repo targets. Agent notes SHALL NOT be stored in canonical graph tables, SHALL NOT add variants to `graph::Epistemic`, and SHALL NOT replace graph-backed source facts.

#### Scenario: Agent records a targeted observation
- **WHEN** an agent creates a note about a file, symbol, concept, test, card target, or existing note
- **THEN** synrepo stores the note as overlay content with `source_store: "overlay"`
- **AND** the note is labeled advisory in every normal user-facing response

#### Scenario: Agent tries to record a free-floating memory
- **WHEN** an agent creates a note without a concrete repo target
- **THEN** synrepo rejects the note
- **AND** no generic session-memory record is persisted

#### Scenario: Note conflicts with graph truth
- **WHEN** an agent note claims a value that differs from a graph-backed fact
- **THEN** synrepo keeps the graph-backed fact authoritative
- **AND** the note remains separate advisory overlay content rather than shadowing the graph field

### Requirement: Require provenance, claim, and evidence fields
synrepo SHALL require each valid agent note to include a target, claim, created-by identity, creation timestamp, confidence value, lifecycle status, and evidence references or an explicit unverified state. Valid fresh notes SHOULD include source hashes or graph revision anchors for drift detection.

#### Scenario: Persist a fully evidenced note
- **WHEN** a note includes target, claim, created-by identity, timestamp, confidence, evidence references, and source hashes
- **THEN** synrepo classifies the note as valid overlay content
- **AND** the note can be returned by explicit note queries

#### Scenario: Persist a note without evidence
- **WHEN** a note includes target, claim, author, timestamp, and confidence but lacks evidence references
- **THEN** synrepo stores the note only as `unverified`
- **AND** normal card responses do not present it as fresh evidence-backed guidance

#### Scenario: Reject or degrade a malformed note
- **WHEN** a note is missing required target, claim, author, timestamp, confidence, or lifecycle status
- **THEN** synrepo classifies the note as invalid or rejects the write
- **AND** invalid notes are withheld from normal card surfaces and visible only through audit queries

### Requirement: Define note lifecycle actions
synrepo SHALL define lifecycle actions for adding, linking, superseding, forgetting, verifying, and listing agent notes. Lifecycle transitions SHALL be auditable and SHALL NOT silently erase prior claims from audit history.

#### Scenario: Supersede an old note
- **WHEN** an agent supersedes an existing note with a newer claim
- **THEN** synrepo marks the old note as `superseded`
- **AND** records a link from the old note to the replacing note

#### Scenario: Forget a note
- **WHEN** a user or authorized agent forgets a note
- **THEN** synrepo hides the note from normal retrieval
- **AND** records a tombstone state available to audit queries unless retention policy later prunes it

#### Scenario: Verify a stale or unverified note
- **WHEN** an agent verifies a note against current source-derived facts
- **THEN** synrepo records verification provenance
- **AND** the note can return to `active` only if its evidence and drift anchors match current source state

### Requirement: Invalidate notes on source drift
synrepo SHALL mark agent notes stale when their source hashes, graph revision anchors, or evidence spans no longer match current deterministic source-derived facts. Drift invalidation SHALL apply only to agent notes and other soft overlay surfaces, never to structural graph facts.

#### Scenario: Source file changes after a note is created
- **WHEN** a note cites a source hash for a file and that file's current hash differs
- **THEN** synrepo marks the note as `stale`
- **AND** normal retrieval labels the note stale rather than fresh

#### Scenario: Evidence node disappears
- **WHEN** a note cites a graph node or evidence span that no longer exists in the current graph
- **THEN** synrepo marks the note stale or invalidated
- **AND** audit output includes the missing evidence reference

#### Scenario: Structural graph facts are refreshed
- **WHEN** source-derived graph facts are reindexed
- **THEN** graph facts remain governed by source invalidation rules
- **AND** no graph fact is decayed, consolidated, or demoted because of agent-note lifecycle policy

### Requirement: Keep note retrieval bounded and explicitly labeled
synrepo SHALL retrieve agent notes only through explicit note surfaces or documented optional card fields. Returned notes SHALL be bounded, target-scoped where possible, and labeled with lifecycle status, confidence, freshness, provenance summary, `source_store: "overlay"`, and advisory status.

#### Scenario: Caller requests notes for a target
- **WHEN** a caller queries notes for a file, symbol, concept, test, or card target
- **THEN** synrepo returns a bounded list of matching notes
- **AND** each note includes advisory source labels and lifecycle state

#### Scenario: Card omits notes by default
- **WHEN** a caller requests a structural card without requesting notes
- **THEN** synrepo returns graph-backed card truth without agent-note content
- **AND** no structural field is influenced by hidden note content

#### Scenario: Card includes optional notes
- **WHEN** a documented card or budget mode explicitly includes agent notes
- **THEN** notes are nested under a distinct advisory field
- **AND** stale, unverified, superseded, and forgotten notes are not presented as fresh active guidance

### Requirement: Prohibit note content from defining source truth
synrepo SHALL NOT use agent notes to compute symbol definitions, file hashes, imports, call graph edges, test mappings, ownership facts, risk scores that claim source truth, or graph-backed card fields. Notes MAY appear as advisory context only when source-labeled and bounded.

#### Scenario: Note claims a test covers a symbol
- **WHEN** an agent note claims that a test covers a symbol
- **THEN** synrepo does not convert that note into a graph-backed test mapping
- **AND** the claim remains advisory unless a deterministic source-derived mechanism later observes the mapping independently

#### Scenario: LLM summarizes repeated notes
- **WHEN** future tooling summarizes note history
- **THEN** the summary is stored as overlay advisory content
- **AND** the summary does not replace source-derived graph facts or evidence-backed mappings

#### Scenario: Search ranks note context
- **WHEN** note content is used to rank optional advisory results
- **THEN** the ranking does not define canonical source truth
- **AND** graph-backed result fields remain derived from source, git, or human-declared graph inputs
