## ADDED Requirements

### Requirement: Define TestSurfaceCard as a graph-derived card type
synrepo SHALL define `TestSurfaceCard` as a structured card that discovers test functions and test modules related to a given scope (file path or directory). All test data SHALL be sourced exclusively from the graph (`source_store: "graph"`). No LLM involvement and no overlay content SHALL appear in a `TestSurfaceCard`. When no tests are found, the card SHALL return an empty `tests` list rather than a spurious result.

#### Scenario: Compile a TestSurfaceCard for a file with associated tests
- **WHEN** `test_surface_card(scope: FilePath, budget)` is called on a file that has test files associated by path convention
- **THEN** the returned card includes at least one `TestEntry` record
- **AND** each `TestEntry` includes the test symbol's `SymbolNodeId`, qualified name, and containing file path
- **AND** `source_store` is `"graph"`

#### Scenario: Compile a TestSurfaceCard for a directory
- **WHEN** `test_surface_card(scope: DirectoryPath, budget)` is called on a directory
- **THEN** the card includes `TestEntry` records for all test symbols found in test files associated with any source file under that directory
- **AND** entries are grouped by source file path

#### Scenario: Compile a TestSurfaceCard when no tests exist
- **WHEN** `test_surface_card(scope, budget)` is called and no test files match any association rule for the given scope
- **THEN** the returned card has an empty `tests` list
- **AND** no error is raised

### Requirement: Define test-discovery heuristics
synrepo SHALL discover tests using two complementary signals, applied in order. A test symbol matches if it satisfies either signal. Both signals MUST hold for the discovery to produce a result.

1. **Symbol-kind signal:** The symbol has `SymbolKind::Test` as assigned by the tree-sitter parser.
2. **File-path signal:** The symbol resides in a file matching one of the following patterns relative to the source file:
   - Sibling test file: same directory, name matches `<stem>_test.rs`, `<stem>_test.go`, `test_<stem>.py`, `<stem>.test.ts`, `<stem>.spec.ts`
   - Parallel test directory: file at `tests/<stem>.rs` or `tests/<stem>.py` when source is at `src/<stem>.rs`
   - Nested test module: file at `<source_dir>/tests/` or `<source_dir>/__tests__/`

#### Scenario: Discover a sibling test file in Rust
- **WHEN** source file is `src/pipeline/git/mod.rs` and a file `src/pipeline/git/mod_test.rs` exists with `SymbolKind::Test` symbols
- **THEN** those test symbols are included as `TestEntry` records associated with the source file

#### Scenario: Discover a parallel test file in Python
- **WHEN** source file is `src/parser/engine.py` and a file `tests/test_engine.py` exists with `SymbolKind::Test` symbols
- **THEN** those test symbols are included as `TestEntry` records

#### Scenario: Discover a spec file in TypeScript
- **WHEN** source file is `src/components/Card.tsx` and a file `src/components/Card.test.tsx` exists with `SymbolKind::Test` symbols
- **THEN** those test symbols are included as `TestEntry` records

#### Scenario: Reject a file with test-like name but no test symbols
- **WHEN** a file matches a test-path pattern but contains no symbols with `SymbolKind::Test`
- **THEN** no `TestEntry` records are emitted from that file
- **AND** the file is not listed in the card

### Requirement: Define the TestEntry structure
synrepo SHALL define each discovered test as a `TestEntry` record containing: `symbol_id` (SymbolNodeId), `qualified_name`, `file_path` (repo-relative), `source_file` (the associated production file path), and `association` (the heuristic that matched: `"symbol_kind"`, `"path_convention"`, or `"both"`).

#### Scenario: TestEntry records the association method
- **WHEN** a test symbol has `SymbolKind::Test` AND resides in a file matching the path convention
- **THEN** its `association` field is `"both"`

#### Scenario: TestEntry records path-only association
- **WHEN** a test symbol does not have `SymbolKind::Test` but resides in a file matching the path convention for the source file
- **THEN** its `association` field is `"path_convention"`

### Requirement: Apply budget-tier truncation to TestSurfaceCard
synrepo SHALL truncate `TestSurfaceCard` content according to the requested budget tier.

#### Scenario: Return a tiny TestSurfaceCard
- **WHEN** a `TestSurfaceCard` is requested at `tiny` budget
- **THEN** the card includes only the count of test files and total test count per source file
- **AND** individual `TestEntry` records are omitted

#### Scenario: Return a normal TestSurfaceCard
- **WHEN** a `TestSurfaceCard` is requested at `normal` budget
- **THEN** the card includes `TestEntry` records with `qualified_name`, `file_path`, and `source_file`
- **AND** `association` and `symbol_id` are included
- **AND** doc comments and signatures are omitted

#### Scenario: Return a deep TestSurfaceCard
- **WHEN** a `TestSurfaceCard` is requested at `deep` budget
- **THEN** each `TestEntry` additionally includes the one-line signature and doc comment truncated to 120 characters
- **AND** for test symbols that have `Calls` edges to production symbols, the card lists the called production symbol IDs in a `covers` field
