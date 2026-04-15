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
