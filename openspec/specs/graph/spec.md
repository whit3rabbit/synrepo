## Purpose
Define the canonical observed-facts graph, including node and edge authority, provenance, and identity-stability behavior.

## Requirements

### Requirement: Define canonical graph entities
synrepo SHALL define the canonical graph in terms of directly observed or human-declared nodes and edges, including file, symbol, and human-backed concept nodes.

#### Scenario: Add a new relationship to the graph
- **WHEN** a contributor proposes a new graph entity or edge type
- **THEN** the graph spec requires direct observation or human declaration as its basis
- **AND** it excludes machine-authored concepts from canonical storage

### Requirement: Carry provenance and epistemic status on graph facts
synrepo SHALL require graph facts to carry provenance and epistemic labels that distinguish parser-observed, git-observed, and human-declared information.

#### Scenario: Inspect a fact's source of authority
- **WHEN** a user or tool inspects a graph row
- **THEN** the row can be traced to the source process and authority level that produced it
- **AND** trust-sensitive behavior can rank competing sources consistently

### Requirement: Define minimum graph provenance fields
synrepo SHALL define the minimum provenance fields required for persisted graph facts, including source revision, producing pass, creation source, and referenced source artifacts.

#### Scenario: Persist a graph-derived artifact
- **WHEN** a graph row is written or surfaced through a user-facing contract
- **THEN** the row includes the minimum provenance required to audit how it was produced
- **AND** missing provenance is treated as an invalid graph artifact rather than an acceptable omission

### Requirement: Define identity instability handling
synrepo SHALL define rename, split, merge, and drift behavior for files and symbols so the graph degrades gracefully under ordinary refactors.

#### Scenario: Refactor a file into two files
- **WHEN** a previously observed file is split across multiple new files
- **THEN** the graph spec defines how identity is preserved or related across the split
- **AND** the system can record drift or findings instead of silently corrupting history

### Requirement: Define graph and git-intelligence boundary
synrepo SHALL define how git-derived facts enter the graph as secondary `git_observed` evidence while keeping repository history enrichments subordinate to parser-observed structure.

#### Scenario: Attach co-change evidence to a file
- **WHEN** git mining detects a meaningful co-change relationship
- **THEN** the graph may store the relationship with `git_observed` authority
- **AND** later consumers can distinguish it from parser-observed structure and overlay inference

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

### Requirement: Populate persisted graph facts automatically from repository state
synrepo SHALL run a deterministic structural compile that discovers eligible repository inputs, parses supported code and configured concept markdown, and writes the resulting canonical graph facts into the persisted graph store without requiring manual graph seeding.

#### Scenario: Initialize a repository and inspect the graph
- **WHEN** a user initializes synrepo in a repository that contains supported source files or configured concept markdown
- **THEN** the structural compile writes the resulting canonical graph facts into `.synrepo/graph/`
- **AND** later `synrepo node` or `synrepo graph query` calls can read those persisted facts without requiring test-only or manual graph insertion

### Requirement: Define the initial structural producer set
synrepo SHALL define the first automatic producer set for the structural compile, including file nodes, symbol nodes, `defines` edges, and human-declared concept nodes from configured concept directories.

#### Scenario: Compile a supported code file and an ADR markdown file
- **WHEN** the structural compile processes a supported code file and a markdown file in a configured concept directory
- **THEN** the code file can produce file nodes, symbol nodes, and `defines` edges
- **AND** the markdown file can produce a concept node and only directly-observed prose facts allowed by the graph contract

### Requirement: Refresh the produced graph slice deterministically
synrepo SHALL refresh the graph facts produced by the initial structural compile deterministically so repeated runs converge on current repository state rather than accumulating duplicate stale facts.

#### Scenario: Re-run the structural compile after a source change
- **WHEN** a user reruns initialization or another structural compile trigger after editing or removing previously observed files
- **THEN** the produced graph slice is refreshed to match current repository state
- **AND** repeated runs over unchanged inputs do not accumulate duplicate nodes or duplicate edges
