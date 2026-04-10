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
- **AND** it does not require LLM synthesis to answer the query

### Requirement: Define encoding and lifecycle boundaries
synrepo SHALL define how encodings, long lines, index compaction, and ignore-policy boundaries are handled so storage behavior remains stable enough for the graph layer to build on.

#### Scenario: Index an ugly repository safely
- **WHEN** the repository contains BOMs, large files, generated trees, or malformed inputs
- **THEN** the substrate contract describes safe handling and refusal behavior
- **AND** it prevents silent transcoding or ambiguous indexing outcomes

### Requirement: Define language adapter support policy
synrepo SHALL define what it means for a language or file class to be fully supported, indexed-only, or unsupported, including the grammar, query, and adapter obligations required for structural support.

#### Scenario: Add support for a new language
- **WHEN** a contributor proposes structural support for another programming language
- **THEN** the substrate contract identifies the grammar source, query expectations, fallback behavior, and support level
- **AND** the change can distinguish full structural support from lexical indexing only

### Requirement: Define grammar maintenance boundaries
synrepo SHALL define grammar version pinning, adapter-layer overrides, and validation expectations for tree-sitter-based language support so parser behavior does not drift silently.

#### Scenario: Upgrade a grammar dependency
- **WHEN** a tree-sitter grammar or query source changes
- **THEN** synrepo applies the declared validation expectations before treating the grammar as supported
- **AND** support does not rely on unversioned query behavior or hidden manual fixes
