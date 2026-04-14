## ADDED Requirements

### Requirement: Define CallPathCard as a graph-derived card type
synrepo SHALL define `CallPathCard` as a structured card that traces execution paths from entry points to a target symbol using backward BFS over `Calls` edges. All path data SHALL be sourced exclusively from the graph (`source_store: "graph"`). No LLM involvement and no overlay content SHALL appear in a `CallPathCard`. When no path is found, the card SHALL return an empty path list rather than a spurious result.

#### Scenario: Compile a CallPathCard for a reachable symbol
- **WHEN** `call_path_card(target: SymbolNodeId, budget)` is called on a symbol that has at least one `Calls` edge chain leading to an entry point
- **THEN** the returned card includes at least one `CallPath` entry
- **AND** each `CallPath` lists `CallPathEdge` records from the entry point symbol to the target
- **AND** `source_store` is `"graph"`

#### Scenario: Compile a CallPathCard for an unreachable symbol
- **WHEN** `call_path_card(target: SymbolNodeId, budget)` is called on a symbol with no `Calls` edges leading to any entry point within the depth budget
- **THEN** the returned card has an empty `paths` list
- **AND** `paths_omitted` is 0
- **AND** no error is raised

#### Scenario: CallPathCard produces no result for symbols in isolation
- **WHEN** the target symbol exists in the graph but has no inbound `Calls` edges
- **THEN** the card returns an empty `paths` list
- **AND** `source_store` is `"graph"`

### Requirement: Define the CallPathEdge structure
synrepo SHALL define each step in a call path as a `CallPathEdge` record containing: `from` (SymbolNodeId + qualified_name), `to` (SymbolNodeId + qualified_name), and `edge_kind` (always `"Calls"` for v1). The first edge in a path SHALL have its `from` field pointing to an entry point symbol.

#### Scenario: CallPathEdge connects two symbols via a Calls edge
- **WHEN** symbol A calls symbol B and this edge appears in a `CallPath`
- **THEN** the edge records `from: A` and `to: B` with `edge_kind: "Calls"`

#### Scenario: First edge originates at an entry point
- **WHEN** a `CallPath` traces from entry point E through intermediate symbols I1, I2 to target T
- **THEN** the first `CallPathEdge` has `from: E` and `to: I1`
- **AND** the last edge has `from: I2` and `to: T`

### Requirement: Bound call path traversal depth
synrepo SHALL bound backward BFS traversal to a default depth of 8 hops. Paths exceeding the depth budget SHALL be truncated: the card includes the partial path up to the depth limit and marks it with `truncated: true` on the final edge.

#### Scenario: Path exceeds depth budget
- **WHEN** the shortest path from an entry point to the target exceeds 8 hops
- **THEN** the card includes a truncated path with `truncated: true` on the final `CallPathEdge`
- **AND** the `from` field of the final truncated edge does not necessarily point to an entry point

#### Scenario: Path fits within depth budget
- **WHEN** a path from an entry point to the target is 5 hops
- **THEN** the card includes the complete path
- **AND** no edge is marked `truncated: true`

### Requirement: Deduplicate and cap call paths
synrepo SHALL deduplicate paths by (entry_point_id, target_id) pair, returning at most 3 distinct paths per pair. When additional paths exist, the card SHALL record the count in `paths_omitted`.

#### Scenario: Multiple paths to the same entry point
- **WHEN** there are 5 distinct paths from entry point E to target T
- **THEN** the card includes 3 paths for the (E, T) pair
- **AND** `paths_omitted` is 2

#### Scenario: Single path to an entry point
- **WHEN** there is exactly one path from entry point E to target T
- **THEN** the card includes that single path
- **AND** `paths_omitted` is 0

### Requirement: Apply budget-tier truncation to CallPathCard
synrepo SHALL truncate `CallPathCard` content according to the requested budget tier.

#### Scenario: Return a tiny CallPathCard
- **WHEN** a `CallPathCard` is requested at `tiny` budget
- **THEN** each path includes only the qualified names of the entry point and target, plus the hop count
- **AND** intermediate symbols and edge details are omitted

#### Scenario: Return a normal CallPathCard
- **WHEN** a `CallPathCard` is requested at `normal` budget
- **THEN** each path includes the full list of `CallPathEdge` records with qualified names and `SymbolNodeId` values
- **AND** signatures, doc comments, and file locations are omitted

#### Scenario: Return a deep CallPathCard
- **WHEN** a `CallPathCard` is requested at `deep` budget
- **THEN** each edge additionally includes the one-line signature and file-relative location for both `from` and `to` symbols
- **AND** the traversal depth increases to 12 hops
