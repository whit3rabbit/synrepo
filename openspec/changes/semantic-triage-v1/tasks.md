## 1. Feature flag and config

- [ ] 1.1 Add `semantic-triage` feature flag to root `Cargo.toml` and `crates/synrepo-mcp/Cargo.toml` (gates `ort` dependency)
- [ ] 1.2 Add config fields to `src/config.rs` with serde defaults: `enable_semantic_triage: bool` (false), `semantic_model: String` ("all-MiniLM-L6-v2"), `embedding_dim: u16` (384), `semantic_similarity_threshold: f64` (0.6)
- [ ] 1.3 Add compatibility advisory when `enable_semantic_triage` changes or `semantic_model`/`embedding_dim` change in an existing config (update `src/store/compatibility/`)
- [ ] 1.4 Verify: `cargo build` (no features) compiles without `ort`; `cargo build --features semantic-triage` links it

## 2. Embedding types and chunk extraction

- [ ] 2.1 Create `src/substrate/embedding/` module directory behind `#[cfg(feature = "semantic-triage")]`
- [ ] 2.2 Define `EmbeddingChunk` struct (chunk text, `ChunkId`, source node reference) and `EmbeddingIndex` trait
- [ ] 2.3 Implement chunk extraction: symbol qualified_name + signature from graph, prose concept full text (truncated 512 tokens)
- [ ] 2.4 Unit tests for chunk extraction covering Rust, Python, TypeScript symbols and prose concepts

## 3. ONNX Runtime model loading and inference

- [ ] 3.1 Implement model resolution: built-in registry (L6, L12, mpnet-base) → download URL + expected dim; string containing `/` → Hugging Face model ID download; absolute `.onnx` path → local file; else → descriptive error
- [ ] 3.2 Implement model download to `.synrepo/index/vectors/model.onnx` on first use (with progress indicator), caching for subsequent runs; support Hugging Face URL pattern `{hf_base}/{model_id}/resolve/main/model.onnx`
- [ ] 3.3 Implement batched inference using `ort`: text input → f16 vector output at configured `embedding_dim`
- [ ] 3.4 Fail fast at index build if model output dimension does not match `embedding_dim` (descriptive error with expected vs actual)
- [ ] 3.5 Implement model load with caching (load once per process, reuse across batches)
- [ ] 3.6 Unit tests for inference with known inputs (deterministic output for identical text), dimension mismatch error, and local model path

## 4. Flat vector index storage

- [ ] 4.1 Implement `FlatVecIndex`: parallel `Vec<f16>` (vectors) + `Vec<ChunkId>` (ids) + metadata (dim, model name)
- [ ] 4.2 Implement persistence to `.synrepo/index/vectors/index.bin` (sequential write, atomic replace)
- [ ] 4.3 Implement load from disk with validation (vector count matches chunk count, stored dim matches configured `embedding_dim`)
- [ ] 4.4 Implement brute-force cosine similarity query: input vector → top-K results by similarity
- [ ] 4.5 Unit tests for index build, query, persistence round-trip, and disposal

## 5. Index build integration

- [ ] 5.1 Wire embedding index build into `synrepo init` substrate pipeline (after lexical index, gated by `enable_semantic_triage`)
- [ ] 5.2 Wire embedding index rebuild into `synrepo reconcile` (replace existing index atomically)
- [ ] 5.3 Add `.synrepo/index/vectors/` to `.synrepo/.gitignore` (generated at init)
- [ ] 5.4 Integration test: `synrepo init` with `enable_semantic_triage = true` produces `.synrepo/index/vectors/` with valid index
- [ ] 5.5 Integration test: deleting `.synrepo/index/vectors/` then running `synrepo reconcile` rebuilds it

## 6. Semantic prefilter integration into cross-link pipeline

- [ ] 6.1 Define `TriageSource` enum (`Deterministic`, `Semantic`) in cross-link types
- [ ] 6.2 Implement semantic prefilter: accept discarded pairs from deterministic prefilter, compute cosine similarity against embedding index, forward pairs above threshold to LLM verification
- [ ] 6.3 Add configurable similarity threshold to config (default: 0.6)
- [ ] 6.4 Wire semantic prefilter into cross-link candidate generation pipeline (after deterministic, before LLM)
- [ ] 6.5 Ensure `source: semantic_triage` is recorded in overlay candidate provenance but no vector data or similarity score is stored
- [ ] 6.6 Integration test: a prose concept with no lexical overlap to a symbol is surfaced as a candidate when semantic triage is enabled
- [ ] 6.7 Integration test: the same scenario with semantic triage disabled produces no additional candidates

## 7. Validation and cleanup

- [ ] 7.1 `make check` passes: fmt, clippy (all targets with `semantic-triage` feature), all tests
- [ ] 7.2 Verify file sizes: all new `.rs` files under 400 lines
- [ ] 7.3 Verify layer imports: `src/substrate/embedding/` does not import from `structure` or higher
- [ ] 7.4 `openspec validate` passes for the change
