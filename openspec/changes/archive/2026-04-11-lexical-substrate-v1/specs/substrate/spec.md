## MODIFIED Requirements

### Requirement: Define deterministic corpus discovery
synrepo SHALL walk configured roots deterministically, respecting repository ignore rules and synrepo redaction rules before any file is admitted to indexing or later structural phases.

#### Scenario: Discover files in a noisy repository
- **WHEN** synrepo scans a repository containing ignored paths, redacted secrets, generated directories, and normal source files
- **THEN** only allowed files are admitted to the discovered set
- **AND** skipped files are excluded for a defined reason rather than by undocumented behavior

### Requirement: Define lexical indexing guarantees
synrepo SHALL build and query a local exact-search index under `.synrepo/index/` for the discovered file set, and `synrepo search` SHALL return deterministic lexical matches from that index.

#### Scenario: Search for an exact term after init
- **WHEN** a user runs `synrepo init` and then `synrepo search <query>`
- **THEN** the query is evaluated against the persisted substrate index
- **AND** the result set is derived from deterministic lexical matching rather than semantic retrieval

### Requirement: Define encoding and lifecycle boundaries
synrepo SHALL skip or refuse files whose size, encoding, or content shape violate the declared substrate policy, including unsupported encodings, LFS pointers, empty files, and oversized inputs.

#### Scenario: Encounter an unsupported file during indexing
- **WHEN** discovery reaches a file that exceeds the size cap or fails the encoding policy
- **THEN** synrepo does not silently index the file
- **AND** the file is classified with a concrete skip reason

## ADDED Requirements

### Requirement: Define initial file-class support policy
synrepo SHALL define the initial supported-code languages and indexed-only file classes used by the Phase 0 substrate.

#### Scenario: Classify a TypeScript file and a YAML file
- **WHEN** discovery classifies a `.ts` file and a `.yaml` file
- **THEN** the TypeScript file is eligible for supported-code handling
- **AND** the YAML file remains indexed-only unless a later capability explicitly upgrades it
