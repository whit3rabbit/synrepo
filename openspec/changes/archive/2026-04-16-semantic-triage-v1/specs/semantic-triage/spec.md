## ADDED Requirements

### Requirement: Opt-in embedding index for cross-link candidate prefiltering
synrepo SHALL provide an opt-in local embedding similarity index that generates cross-link candidate pairs supplementary to the deterministic name-match prefilter. The embedding model SHALL be configurable via the `semantic_model` config field (accepts a built-in model name or an absolute path to a local `.onnx` file) with a default of `all-MiniLM-L6-v2`. The `embedding_dim` config field SHALL declare the model's output dimension (default: 384); a mismatch between this value and the model's actual output SHALL fail at index build time with a descriptive error. No network calls SHALL occur at inference time. The index SHALL be stored under `.synrepo/index/vectors/`, SHALL be gitignored, and SHALL be fully rebuildable from the canonical graph alone (deleting it causes no data loss). The embedding index SHALL NOT be queryable through any MCP tool or CLI surface other than the cross-link candidate generation pipeline.

#### Scenario: Build the embedding index during init
- **WHEN** `synrepo init` runs with `enable_semantic_triage = true` in config
- **THEN** the substrate index build embeds all symbol qualified names + signatures and all prose concept texts into vectors stored in `.synrepo/index/vectors/`
- **AND** the index is usable for cosine similarity queries without re-embedding

#### Scenario: Use a non-default model
- **WHEN** `semantic_model` is set to a built-in model name (e.g., `all-mpnet-base-v2`) with matching `embedding_dim` (e.g., 768)
- **THEN** the specified model is downloaded and used for embedding, and vectors are stored at the declared dimension
- **AND** the vector index and queries use the correct dimensionality

#### Scenario: Model dimension mismatch fails fast
- **WHEN** `embedding_dim` is set to 384 but the model outputs 768-dimensional vectors
- **THEN** index build fails with a descriptive error naming the expected and actual dimensions
- **AND** no partial index is written to disk

#### Scenario: Use a Hugging Face model
- **WHEN** `semantic_model` is set to a string containing `/` (e.g., `intfloat/e5-base-v2`) that does not match a built-in registry name and `embedding_dim` is set to match the model's output (e.g., 768)
- **THEN** the system downloads the ONNX variant from Hugging Face and caches it at `.synrepo/index/vectors/model.onnx`
- **AND** subsequent reconciles reuse the cached file without re-downloading (unless the model name changes)

#### Scenario: Use a local ONNX file
- **WHEN** `semantic_model` is set to an absolute path ending in `.onnx` and `embedding_dim` matches the model's output
- **THEN** the local model is used without downloading
- **AND** the index is built identically to a built-in model

#### Scenario: Invalid model identifier
- **WHEN** `semantic_model` is not a built-in name, does not contain `/`, and does not end in `.onnx`
- **THEN** init or reconcile fails with a descriptive error explaining the three accepted formats (built-in name, Hugging Face ID, local .onnx path)

#### Scenario: Skip embedding index when disabled
- **WHEN** `enable_semantic_triage` is `false` or absent in config
- **THEN** no embedding index is built, no model is downloaded, and no vectors are stored
- **AND** the cross-link candidate pipeline runs with the deterministic prefilter only

#### Scenario: Rebuild the embedding index on reconcile
- **WHEN** `synrepo reconcile` runs with `enable_semantic_triage = true`
- **THEN** the embedding index is rebuilt from the current graph content, replacing the previous index
- **AND** the rebuild is atomic (new index replaces old; partial writes do not corrupt)

#### Scenario: Vector index is disposable
- **WHEN** the `.synrepo/index/vectors/` directory is deleted
- **THEN** `synrepo init` or `synrepo reconcile` rebuilds it completely from the graph
- **AND** no data in the canonical graph or overlay is lost or degraded

### Requirement: Semantic similarity prefilter feeds cross-link candidate pipeline
synrepo SHALL use the embedding index as a supplementary prefilter that runs after the deterministic name-match prefilter and before LLM evidence extraction. The semantic prefilter SHALL receive the pairs discarded by the deterministic prefilter, compute cosine similarity between their vector representations, and forward pairs exceeding the configured similarity threshold to the LLM verification stage. Semantic candidates SHALL pass through the identical evidence extraction, confidence scoring, and overlay storage pipeline as deterministic candidates. No vector data, similarity scores, or embedding metadata SHALL be written to the canonical graph or the overlay store.

#### Scenario: Semantic prefilter catches a pair the deterministic prefilter missed
- **WHEN** a (prose concept, symbol) pair was discarded by the deterministic prefilter (zero name overlap) but their cosine similarity exceeds the configured threshold
- **THEN** the semantic prefilter forwards the pair to LLM evidence extraction
- **AND** if the LLM produces verified evidence, the candidate is stored in the overlay with the same confidence scoring as a deterministic candidate
- **AND** the candidate's provenance records `semantic_triage` as the source

#### Scenario: Semantic prefilter produces no candidates
- **WHEN** all discarded pairs have cosine similarity below the threshold
- **THEN** no additional pairs are forwarded to LLM verification
- **AND** the candidate pipeline output is identical to deterministic-only mode

#### Scenario: Vectors never enter the graph or overlay
- **WHEN** a semantic candidate is promoted to the graph through the human review workflow
- **THEN** the promoted graph edge carries `Epistemic::HumanDeclared` provenance with no embedding metadata
- **AND** the overlay candidate row records `source: semantic_triage` but does not store the similarity score or vector data

### Requirement: Config and feature flag gating for semantic triage
synrepo SHALL gate semantic triage on both a Cargo feature flag (`semantic-triage`) and runtime config fields. The feature flag SHALL gate compilation of the embedding module and its ONNX Runtime dependency. The config fields SHALL gate runtime behavior:
- `enable_semantic_triage` (bool, default `false`): when `false`, no embedding index is built and no semantic prefiltering occurs.
- `semantic_model` (string, default `"all-MiniLM-L6-v2"`): built-in model name or absolute path to a local `.onnx` file.
- `embedding_dim` (u16, default `384`): expected model output dimension.
- `semantic_similarity_threshold` (float, default `0.6`): cosine similarity threshold for the semantic prefilter.

Changing `enable_semantic_triage` from `false` to `true` SHALL trigger a compatibility advisory (not a rebuild). Changing `semantic_model` or `embedding_dim` SHALL invalidate the existing vector index and require a reconcile. The model file SHALL be downloaded on first use when both the feature flag is compiled in and `enable_semantic_triage` is `true`.

#### Scenario: Default build excludes semantic triage
- **WHEN** synrepo is built without the `semantic-triage` feature flag
- **THEN** no ONNX Runtime dependency is linked, no embedding module is compiled, and `enable_semantic_triage` has no effect at runtime

#### Scenario: Feature compiled but config disabled
- **WHEN** synrepo is built with the `semantic-triage` feature but `enable_semantic_triage = false`
- **THEN** the embedding module is compiled but no model is downloaded, no index is built, and cross-link generation uses deterministic prefiltering only

#### Scenario: Config change triggers advisory
- **WHEN** `enable_semantic_triage` is changed from `false` to `true` in an existing `.synrepo/config.toml`
- **THEN** `synrepo upgrade` reports an advisory that semantic triage is newly enabled and a reconcile is needed to build the index
- **AND** no graph rebuild or data migration is required

#### Scenario: Model change invalidates vector index
- **WHEN** `semantic_model` or `embedding_dim` is changed in an existing config
- **THEN** `synrepo upgrade` reports an advisory that the vector index is stale and a reconcile is needed
- **AND** the next reconcile rebuilds the vector index with the new model
