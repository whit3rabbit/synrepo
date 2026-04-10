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
