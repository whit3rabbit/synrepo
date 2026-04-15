## MODIFIED Requirements

### Requirement: Define identity instability handling
synrepo SHALL resolve identity for files that disappear across compile cycles using a 5-step cascade: content-hash rename, symbol-set split, symbol-set merge, git rename fallback, and breakage. Each step produces a typed `IdentityResolution` and consumes the disappeared file so it is not reconsidered by later steps.

#### Scenario: Refactor a file into two files
- **WHEN** a file disappears and two new files contain subsets of its qualified symbol names with Jaccard overlap >= 0.4 against the old file
- **THEN** synrepo preserves the old `FileNodeId` on the highest-overlap new file, creates a new `FileNode` for the lower-overlap file, and emits a `SplitFrom` edge from the secondary to the primary with `Epistemic::ParserObserved`
- **AND** both new files carry the old file's symbols via existing `Defines` edges without orphaning any symbol

#### Scenario: Merge two files into one
- **WHEN** two or more files disappear and one new file's qualified symbol names have Jaccard overlap >= 0.5 against each disappeared file
- **THEN** synrepo creates a new `FileNode` for the merged file, emits `MergedFrom` edges from the new node to each disappeared node with `Epistemic::ParserObserved`, and marks the disappeared nodes as superseded

#### Scenario: Rename detected by content hash
- **WHEN** a file disappears and one new file has an identical content hash
- **THEN** synrepo preserves the old `FileNodeId`, appends the new path to `path_history`, and emits no additional identity edges
- **AND** all existing edges targeting the old node remain valid

#### Scenario: No identity match resolves
- **WHEN** a file disappears and no new file matches by content hash, symbol-set overlap, or git rename
- **THEN** synrepo classifies the disappearance as `Breakage`, logs the old `FileNodeId` and reason, and the old node remains in the graph for downstream repair detection

## ADDED Requirements

### Requirement: Compute structural drift scores for graph edges
synrepo SHALL compute a drift score in `[0.0, 1.0]` for each graph edge on every structural compile cycle, comparing the structural fingerprint of source artifacts at edge-creation time versus current state.

#### Scenario: Edge links two unchanged artifacts
- **WHEN** both artifacts connected by an edge have identical structural fingerprints since the edge was created
- **THEN** the drift score for that edge SHALL be 0.0

#### Scenario: Edge links to a deleted artifact
- **WHEN** one artifact connected by an edge no longer exists in the graph
- **THEN** the drift score for that edge SHALL be 1.0

#### Scenario: Edge links artifacts with signature changes
- **WHEN** the target artifact's structural fingerprint has changed (symbol signatures added, removed, or modified) but the artifact still exists
- **THEN** the drift score SHALL be in (0.0, 1.0) proportional to the Jaccard distance between the old and new signature sets

#### Scenario: Persist drift scores across compile cycles
- **WHEN** a structural compile cycle completes
- **THEN** drift scores for all edges SHALL be persisted in a sidecar table keyed by edge ID and revision
- **AND** scores from previous revisions SHALL be truncated at the start of each new cycle
