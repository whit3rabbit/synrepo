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

### Requirement: Expose a reader-consistent snapshot over multi-query reads
The canonical graph store SHALL expose a paired `begin_read_snapshot` / `end_read_snapshot` API so that any logical operation issuing multiple queries through a single handle observes exactly one committed epoch for the entire scope. Nested snapshots on the same handle SHALL share the outermost epoch and MUST NOT error, so reader wrappers (MCP handlers, CLI commands) compose safely with inner wrappers (card compilation). The snapshot API SHALL NOT require writers to acquire additional locks and SHALL NOT make reads block on the writer.

#### Scenario: Reader is isolated from a concurrent writer commit
- **WHEN** a reader opens a read snapshot, issues one query, a writer commits through a different handle, and the reader issues a follow-up query inside the same snapshot
- **THEN** the follow-up query observes the pre-commit epoch rather than a mix of pre- and post-commit state
- **AND** the writer's commit becomes visible to the reader only after the snapshot is ended

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

### Requirement: Extract symbol signature and doc comment during structural parse
synrepo SHALL extract a one-line signature and the immediately-preceding doc comment for each matched symbol node during structural parse, for all supported languages (Rust, Python, TypeScript/TSX), and SHALL persist the extracted values on the corresponding `SymbolNode`.

#### Scenario: Parse a documented Rust function
- **WHEN** the structural compile processes a Rust source file containing a `///`-commented function
- **THEN** the resulting `SymbolNode` has a non-None `signature` field containing the function declaration text up to the opening brace
- **AND** the `doc_comment` field contains the concatenated `///` comment lines immediately preceding the function

#### Scenario: Parse an undocumented symbol
- **WHEN** the structural compile processes a symbol that has no preceding doc comment
- **THEN** the resulting `SymbolNode` has `doc_comment: None`
- **AND** `signature` is still populated from the declaration text if the language supports it

#### Scenario: Parse a Python function with a docstring
- **WHEN** the structural compile processes a Python function whose body begins with a string literal
- **THEN** the `doc_comment` field on the resulting `SymbolNode` contains that string literal's text
- **AND** `signature` contains the `def` line up to and including the closing `:`

#### Scenario: Parse a TypeScript function with a JSDoc comment
- **WHEN** the structural compile processes a TypeScript function preceded by a `/** */` block comment
- **THEN** the `doc_comment` field on the resulting `SymbolNode` contains the JSDoc content
- **AND** `signature` contains the function declaration up to the opening brace

### Requirement: Stabilize file identity across content edits
synrepo SHALL preserve `FileNodeId` across in-place content edits. A content-hash change SHALL advance the `content_hash` version field on `FileNode` without triggering node deletion. Physical deletion of a file node SHALL be reserved for files genuinely absent from the repository after the identity cascade and for the compaction maintenance pass.

#### Scenario: Edit a file in place and re-compile
- **WHEN** a user edits a file's content and the structural compile runs
- **THEN** the `FileNode` retains its original `FileNodeId`
- **AND** the `content_hash` field is updated to reflect the new content
- **AND** no symbols or edges are cascade-deleted as a side effect of the content change

#### Scenario: Delete a file from the repository
- **WHEN** a file is removed from the repository and the identity cascade finds no rename, split, or merge match
- **THEN** the file node and all its owned observations are physically deleted
- **AND** drift scoring records the deletion as endpoint score 1.0

### Requirement: Carry observation ownership on parser-emitted facts
synrepo SHALL record `owner_file_id` on every parser-emitted symbol and edge, identifying the file whose parse pass produced the observation. Recompiling a file SHALL retire only observations owned by that file, leaving observations owned by other files untouched.

#### Scenario: Recompile one file in a multi-file graph
- **WHEN** the structural compile reprocesses file A while file B is unchanged
- **THEN** only observations with `owner_file_id = A` are subject to retirement
- **AND** observations owned by file B retain their `last_observed_rev` and `retired_at_rev` state

### Requirement: Soft-retire non-observed facts instead of deleting them
synrepo SHALL mark parser-observed symbols and edges as retired (`retired_at_rev = current_revision`) when they are not re-emitted during a structural compile pass. Retired facts SHALL remain physically present until removed by the compaction maintenance pass. Re-emission of a previously retired fact SHALL clear `retired_at_rev` and advance `last_observed_rev`.

#### Scenario: A symbol is removed from a file
- **WHEN** a file edit removes a previously observed symbol and the structural compile runs
- **THEN** the symbol's `retired_at_rev` is set to the current compile revision
- **AND** the symbol remains queryable via `all_edges()` for drift scoring
- **AND** `active_edges()` and `symbols_for_file()` exclude the retired symbol

#### Scenario: A previously retired symbol reappears
- **WHEN** a file edit re-introduces a symbol that was retired in a prior compile revision
- **THEN** the symbol's `retired_at_rev` is cleared
- **AND** `last_observed_rev` is set to the current compile revision

### Requirement: Respect epistemic ownership boundaries during retirement
synrepo SHALL NOT retire human-declared facts (`Epistemic::HumanDeclared`) during a parser compile pass. Only the emitter class that produced a fact (parser, git, human) may retire it. Parser passes SHALL retire only `ParserObserved` observations.

#### Scenario: A parser pass encounters a human-declared Governs edge
- **WHEN** the structural compile refreshes a file that has an incident `Governs` edge with `Epistemic::HumanDeclared`
- **THEN** the `Governs` edge is not retired regardless of whether it was re-emitted by the parser
- **AND** only `ParserObserved` edges owned by the file are subject to retirement

### Requirement: Track compile revisions as a monotonic counter
synrepo SHALL maintain a `compile_revisions` table with a monotonically increasing integer revision counter. Each structural compile SHALL increment the counter. All observation-window fields (`last_observed_rev`, `retired_at_rev`) SHALL reference this counter.

#### Scenario: Two compiles within the same git commit
- **WHEN** a user runs `synrepo init` twice without any git commits between runs
- **THEN** each compile increments the revision counter independently
- **AND** observation windows distinguish the two compiles by their revision numbers

### Requirement: Compact retired observations as a maintenance operation
synrepo SHALL provide a compaction pass that physically deletes retired symbols, edges, and associated sidecar data older than a configurable retention window (`retain_retired_revisions`, default 10). Compaction SHALL run during `synrepo sync` and `synrepo upgrade --apply`, never during the hot reconcile path.

#### Scenario: Compact after the retention window expires
- **WHEN** `synrepo sync` runs and retired observations exist with `retired_at_rev < current_rev - retain_retired_revisions`
- **THEN** those symbols and edges are physically deleted from the store
- **AND** associated `edge_drift` and `file_fingerprints` rows older than the retention window are also removed
- **AND** active (non-retired) observations are never deleted by compaction

