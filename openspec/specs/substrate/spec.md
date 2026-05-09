## Purpose
Define the deterministic lexical and file-handling substrate that higher synrepo layers rely on for discovery, indexing, and exact lookup.
## Requirements
### Requirement: Define deterministic corpus discovery
synrepo SHALL define a deterministic substrate contract for discovering text inputs, applying ignore rules, and selecting supported file classes before higher layers consume repository content.

#### Scenario: Scan a repository with mixed content
- **WHEN** synrepo encounters code, prose, generated files, binaries, and ignored paths
- **THEN** the substrate contract defines which inputs are indexed, skipped, or partially handled
- **AND** higher-level behavior can rely on stable discovery rules

### Requirement: Define lexical indexing guarantees
synrepo SHALL define lexical indexing as an exact search substrate that supports name lookup, fallback retrieval, and evidence verification for later overlay behavior.

#### Scenario: Resolve a name lookup without semantic search
- **WHEN** an agent or internal component needs lexical fallback
- **THEN** the substrate contract guarantees deterministic exact-search behavior
- **AND** it does not require Explain to answer the query

### Requirement: Keep root-aware lexical lookup deterministic
The primary checkout SHALL be indexed through syntext. Discovered non-primary roots such as linked worktrees SHALL be searchable through a bounded direct scan that uses the same discovery classification, path filters, file type filters, exclude filters, case-sensitivity option, and result limits as primary-root lexical search. Non-primary roots SHALL NOT be inserted into the primary syntext index while syntext stores paths relative to the primary repo root.

#### Scenario: Search finds a worktree-only token
- **WHEN** a linked worktree is included in discovery and contains a unique token
- **THEN** root-aware lexical lookup returns the worktree match
- **AND** the match includes the worktree root discriminator without exposing the absolute worktree path

### Requirement: Allow incremental lexical index maintenance for watch mode
synrepo SHALL allow watch-driven lexical index maintenance from a bounded touched-path set when the watch service has a trustworthy coalesced batch of repo-relative file changes. The incremental path SHALL skip `.synrepo/` and `.git/`, ignore directories, respect configured roots and redaction policy, and evict entries whose paths are now out of policy or deleted. When no trustworthy touched-path set exists, or when the underlying syntext index is missing, corrupt, lock-conflicted, or overlay-full, synrepo SHALL fall back to a full rebuild.

#### Scenario: Watch service reconciles a bounded touched-path batch
- **WHEN** the watch service completes a coalesced reconcile with a concrete touched-path set
- **THEN** the substrate contract permits incremental syntext updates for changed and deleted files
- **AND** `.synrepo/` and `.git/` runtime noise does not dirty the repo lexical index

#### Scenario: Startup or manual reconcile has no trustworthy touched-path set
- **WHEN** synrepo runs startup reconcile or an operator-triggered reconcile without a concrete touched-path batch
- **THEN** the substrate contract requires a conservative full lexical rebuild instead of incremental sync

### Requirement: Define encoding and lifecycle boundaries
synrepo SHALL define how encodings, long lines, index compaction, and ignore-policy boundaries are handled so storage behavior remains stable enough for the graph layer to build on.

#### Scenario: Index an ugly repository safely
- **WHEN** the repository contains BOMs, large files, generated trees, or malformed inputs
- **THEN** the substrate contract describes safe handling and refusal behavior
- **AND** it prevents silent transcoding or ambiguous indexing outcomes

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

### Requirement: Define grammar maintenance boundaries
synrepo SHALL define grammar version pinning, adapter-layer overrides, and validation expectations for tree-sitter-based language support so parser behavior does not drift silently.

#### Scenario: Upgrade a grammar dependency
- **WHEN** a tree-sitter grammar or query source changes
- **THEN** synrepo applies the declared validation expectations before treating the grammar as supported
- **AND** support does not rely on unversioned query behavior or hidden manual fixes

### Requirement: Provide bounded hybrid search over lexical and embedding indexes
The substrate SHALL provide a hybrid-search helper that combines syntext lexical top 100 with embedding vector top 50 using reciprocal rank fusion with `k = 60`. The helper SHALL be read-only and SHALL NOT reconcile, rebuild, download models, or mutate indexes.

#### Scenario: Hybrid search has no semantic index
- **WHEN** the vector index or local model assets are unavailable
- **THEN** callers can fall back to lexical search without treating the absence as corpus corruption

### Requirement: Build richer symbol embedding text
When semantic triage builds symbol chunks, each symbol chunk SHALL include qualified name, symbol kind, file path when available, signature when available, and doc comment when available.

#### Scenario: Symbol has signature and docs
- **WHEN** an embedding chunk is extracted for a documented symbol
- **THEN** the chunk text includes the symbol's qualified name, kind, file path, signature, and doc comment
- **AND** changing the chunk text format invalidates the prior vector index format
