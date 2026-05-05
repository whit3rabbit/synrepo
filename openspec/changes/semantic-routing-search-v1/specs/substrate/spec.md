## ADDED Requirements

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
