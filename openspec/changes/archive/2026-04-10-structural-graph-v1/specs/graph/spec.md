## ADDED Requirements

### Requirement: Persist canonical graph facts in the graph store
synrepo SHALL persist canonical graph nodes and edges in a sqlite-backed graph store under `.synrepo/graph/`, and each persisted row SHALL retain its stable ID, epistemic label, and provenance metadata.

#### Scenario: Round-trip a persisted file, symbol, concept, and edge
- **WHEN** synrepo writes canonical graph facts to the graph store and later reads them back
- **THEN** file, symbol, concept, and edge records retain the same stable identifiers they were written with
- **AND** each record retains its epistemic status and minimum provenance fields without dropping source authority information

### Requirement: Admit concept nodes only from configured human-authored concept directories
synrepo SHALL create concept nodes only from human-authored markdown sources located in configured concept directories, and SHALL reject concept-node creation from machine-authored or out-of-scope inputs.

#### Scenario: Inspect a markdown file inside and outside concept directories
- **WHEN** synrepo evaluates a human-authored markdown file in `docs/adr/` and another markdown file outside the configured concept directories
- **THEN** only the file in the configured concept directory is eligible to produce a concept node
- **AND** the out-of-scope markdown file does not create a concept node in the canonical graph

### Requirement: Support direct graph inspection for persisted facts
synrepo SHALL support direct inspection of persisted graph facts through node lookup, graph statistics, and simple edge-filtered traversals over the canonical graph store.

#### Scenario: Inspect a stored node and its relationships
- **WHEN** a user requests a stored node by ID or asks for related edges of a persisted node
- **THEN** synrepo returns the stored node metadata or matching related edges from the graph store
- **AND** the response is derived from persisted graph facts rather than inferred overlay content
