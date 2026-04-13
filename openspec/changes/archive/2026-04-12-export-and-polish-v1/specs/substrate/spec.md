## ADDED Requirements

### Requirement: Add Go as a fully-supported structural language
synrepo SHALL treat Go as a fully-supported structural language, providing tree-sitter-based symbol extraction, call/import edge production, and signature extraction at the same level as the existing Rust, Python, and TypeScript/TSX adapters.

#### Scenario: Index a Go repository
- **WHEN** synrepo indexes a repository containing `.go` source files
- **THEN** the structural compile extracts `SymbolNode` records for functions, methods, types, interfaces, constants, and variables
- **AND** `signature` and `doc_comment` fields are populated from Go declarations and `//` doc comments
- **AND** `Calls` and `Imports` edges are produced by stage 4 cross-file resolution

#### Scenario: Go grammar version changes
- **WHEN** the `tree-sitter-go` grammar dependency version changes
- **THEN** a grammar validation test confirms expected symbol counts and edge types on a known Go fixture before the change is accepted as supported
- **AND** the grammar version is pinned in `Cargo.toml` rather than resolved to latest

#### Scenario: Go file encountered before adapter is initialized
- **WHEN** synrepo encounters a `.go` file and the Go adapter fails to initialize
- **THEN** the file is classified as `SupportedCode { language: Go }` and indexed lexically
- **AND** structural extraction is skipped for that file with a warning rather than a hard failure

## MODIFIED Requirements

### Requirement: Define language adapter support policy
synrepo SHALL define what it means for a language or file class to be fully supported, indexed-only, or unsupported, including the grammar, query, and adapter obligations required for structural support. Fully-supported languages are: Rust, Python, TypeScript, TSX, and Go. All other languages are indexed-only unless explicitly promoted in a future change.

#### Scenario: Add support for a new language
- **WHEN** a contributor proposes structural support for another programming language
- **THEN** the substrate contract identifies the grammar source, query expectations, fallback behavior, and support level
- **AND** the change can distinguish full structural support from lexical indexing only

#### Scenario: Encounter a file in a lexical-only language
- **WHEN** synrepo encounters a file in a language that is indexed-only (e.g., Java, C, Ruby)
- **THEN** the file is added to the substrate index for lexical search
- **AND** no symbol extraction or structural edges are produced
- **AND** the file's `FileNode` is created with `FileClass::TextCode` or `SupportedCode { language }` at the appropriate classification level
