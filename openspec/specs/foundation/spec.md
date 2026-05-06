## Purpose
Define synrepo's enduring product mission, trust boundaries, operating modes, and the rule that OpenSpec is a planning layer rather than runtime truth.
## Requirements
### Requirement: Define synrepo's product wedge
synrepo SHALL define itself as a context compiler for AI coding agents whose primary product is card-based context delivery rather than generated documentation or ontology browsing.

#### Scenario: Orient an agent on a fresh repository
- **WHEN** a contributor reads the foundation spec to understand synrepo's purpose
- **THEN** the spec describes cards as the primary user-facing abstraction
- **AND** it states the desired outcomes of fewer blind reads, fewer wrong-file edits, lower token burn, and faster orientation

### Requirement: Preserve trust separation between graph and overlay
synrepo SHALL treat the graph as canonical for parser-observed, git-observed, and human-declared facts, while machine-authored commentary and proposed links remain in a separate overlay.

#### Scenario: Distinguish canonical and supplemental data
- **WHEN** a future change defines new storage or retrieval behavior
- **THEN** the change can trace graph-backed facts and overlay-backed content to separate roles
- **AND** it cannot claim that overlay output becomes canonical without new human-declared source material

### Requirement: Support both auto and curated modes
synrepo SHALL support low-ceremony auto mode for vibe coders and curated mode for teams with maintained rationale sources, while preserving the same trust model in both modes.

#### Scenario: Compare operating modes
- **WHEN** a contributor defines onboarding or review behavior
- **THEN** the spec distinguishes auto mode from curated mode by workflow and review surface
- **AND** it does not redefine the underlying graph versus overlay boundary per mode

### Requirement: Keep OpenSpec as a planning layer
synrepo SHALL use OpenSpec to capture enduring product behavior in `openspec/specs/` and active work in `openspec/changes/`, without treating OpenSpec artifacts as runtime truth or the end-user product surface.

#### Scenario: Place a new artifact in the repository
- **WHEN** a contributor needs to record a product contract, proposed work item, or runtime export
- **THEN** the artifact location can be chosen by this rule set
- **AND** OpenSpec artifacts are used for planning and change control rather than runtime retrieval

### Requirement: Preserve the product boundary between code memory and task memory
synrepo SHALL manage code memory and bounded operational memory only. It SHALL NOT manage generic task memory, chat memory, or cross-session agent memory. Any workflow handoff surface MUST be derived from graph and overlay state rather than stored as a canonical planning record.

#### Scenario: Propose a new storage surface
- **WHEN** a future change introduces a new store or mutable surface
- **THEN** the change is rejected if the store would hold assignments, statuses, comments, or chat logs as authoritative data
- **AND** handoff-style surfaces must be specified as read-only projections of existing graph, overlay, or state content

### Requirement: Maintain uniform agent doctrine across surfaces
synrepo SHALL describe the same default agent path in every agent-facing surface: search or entry-point discovery, then `tiny` cards, then `normal` cards, then `deep` cards only before edits or when exact source or body details matter, with overlay commentary treated as advisory and freshness requested explicitly when it matters.

#### Scenario: Ship or update an agent-facing surface
- **WHEN** a change adds or modifies `skill/SKILL.md`, a generated agent shim, an MCP tool description, a CLI example in shipped docs, or bootstrap output
- **THEN** the change preserves the single escalation path and do-not rules
- **AND** it does not introduce a competing default path per agent target

### Requirement: Bound semantic compaction to soft surfaces
synrepo SHALL apply semantic compaction, summarization, or retention policies only to overlay content, findings, cross-link candidates and audit rows, and recent operational history. Canonical graph data SHALL NOT be compacted semantically or replaced by summaries.

#### Scenario: Add or extend a compaction policy
- **WHEN** a change introduces or modifies a compaction, pruning, or summarization rule
- **THEN** the rule's scope is restricted to overlay, findings, or state directories
- **AND** the change cannot transform, drop, or summarize canonical graph rows beyond existing dead-node reference cleanup

### Requirement: Treat workflow handoffs as derived surfaces
synrepo MAY emit structured handoff or next-action items, but every emitted item SHALL carry `source_store`, `epistemic_status`, and freshness, and SHALL be reproducible from the persisted inputs it was derived from.

#### Scenario: Add or extend a handoff surface
- **WHEN** a change defines a new action-oriented surface (e.g. `synrepo_next_actions`, `synrepo handoffs`)
- **THEN** the surface reads only from graph, overlay, repair state, recent-activity state, and git-intelligence state
- **AND** losing the emitted list and regenerating it produces equivalent content up to ordering within a tied severity tier

### Requirement: Publish an immutable graph snapshot after structural compile
synrepo SHALL rebuild and atomically publish an immutable in-memory graph snapshot after each successful structural compile commit. The snapshot SHALL be derived from committed SQLite graph state, and readers SHALL observe either the previously published snapshot or the newly published snapshot, never a partial compile.

#### Scenario: Publish a new snapshot after a successful compile
- **WHEN** stages 1 through 7 of the structural pipeline commit successfully
- **THEN** stage 8 rebuilds a full in-memory graph snapshot from the committed graph state and atomically publishes it
- **AND** read-path consumers can observe a consistent snapshot epoch for the duration of a request

#### Scenario: Retain the previous snapshot when compile fails
- **WHEN** a structural compile fails before the SQLite commit completes
- **THEN** synrepo does not publish a replacement snapshot
- **AND** readers continue to observe the last successfully published snapshot

### Requirement: Keep SQLite authoritative when snapshots are enabled
synrepo SHALL treat the in-memory graph snapshot as a derived optimization, not as the authoritative store. Mutations SHALL continue to write SQLite first, and the snapshot SHALL remain rebuildable from SQLite alone after process restart.

#### Scenario: Rebuild snapshot state after process restart
- **WHEN** a new synrepo process starts with no in-memory snapshot populated yet
- **THEN** it can rebuild and republish the snapshot from the persisted SQLite graph
- **AND** no canonical graph fact depends on persisting the in-memory snapshot itself

### Requirement: Bound snapshot memory with operator controls
synrepo SHALL expose an operator control for the maximum in-memory snapshot size and SHALL warn when a published snapshot exceeds that advisory ceiling. Setting the ceiling to `0` SHALL disable snapshot publication and keep read paths on SQLite.

#### Scenario: Snapshot exceeds the advisory ceiling
- **WHEN** stage 8 builds a snapshot larger than the configured `max_graph_snapshot_bytes`
- **THEN** synrepo emits a warning naming the snapshot size and graph counts
- **AND** it still publishes the snapshot unless the configured ceiling is `0`

#### Scenario: Operator disables snapshot publication
- **WHEN** `max_graph_snapshot_bytes` is set to `0`
- **THEN** synrepo skips publishing the in-memory snapshot
- **AND** read-path consumers fall back to SQLite-backed graph reads

### Requirement: Describe layered context artifacts
The foundation SHALL describe synrepo as a local code-context compiler that turns repository files into canonical graph facts, compiles those facts into code artifacts, bundles artifacts into task contexts, and serves those contexts through cards and MCP. This framing SHALL preserve cards as the current primary delivery packet while clarifying that the graph is infrastructure and artifacts and contexts are the product abstraction.

#### Scenario: Reader learns synrepo's product model
- **WHEN** a contributor reads the foundation document or foundation spec
- **THEN** they can identify the layers `repo files`, `graph facts`, `code artifacts`, `task contexts`, and `cards/MCP`
- **AND** they can tell which layers are canonical, compiled, bundled, or delivered

#### Scenario: Runtime behavior remains unchanged
- **WHEN** the framing language is updated
- **THEN** no foundation requirement implies a new MCP tool, new storage surface, new background job, or changed trust boundary

