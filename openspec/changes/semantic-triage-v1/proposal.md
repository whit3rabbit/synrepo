## Why

Cross-link candidate generation currently relies on a deterministic prefilter (token name/identifier overlap + graph distance cutoff) before LLM verification. This misses candidates where prose phrasing shares no lexical tokens with the symbol name but describes the same concept semantically. ROADMAP.md Track K scopes "bounded semantic linking" and §9.11 explicitly permits embeddings as a bounded, opt-in candidate generator for overlay cross-links. This change adds an opt-in local embedding similarity prefilter that augments the existing deterministic triage, catching these missed pairs without making vectors a core dependency or polluting the canonical graph.

## What Changes

- Add an opt-in embedding similarity prefilter stage between the existing deterministic prefilter and the LLM evidence extraction stage, using a local ONNX runtime with all-MiniLM-L6-v2.
- Embed text chunks (symbol qualified names + signatures, prose concept text) at index time into a bounded in-memory vector index stored alongside `.synrepo/index/`.
- The embedding prefilter produces candidate (prose, symbol) pairs that the deterministic prefilter missed, feeding them into the same LLM evidence extraction and confidence scoring pipeline as deterministic candidates.
- No vectors, embeddings, or similarity scores enter the canonical graph or the overlay store. The vector index is a derived, disposable cache: deleting it causes no data loss, and `synrepo init` rebuilds it from scratch.
- Add `enable_semantic_triage` config field (default: `false`) and `--semantic` flag on `synrepo init`/`synrepo reconcile`.

## Capabilities

### New Capabilities

- `semantic-triage`: Local embedding-based similarity prefilter for cross-link candidate generation. Covers the embedding pipeline, vector index lifecycle, config gating, and the invariant that vectors never leave the index.

### Modified Capabilities

- `overlay-links`: The prefilter requirement expands to allow semantic similarity as a second triage source alongside deterministic name matching, subject to the same LLM verification and confidence scoring pipeline.

## Impact

- **Dependencies**: Adds `ort` (ONNX Runtime Rust bindings) and downloads the configured embedding model at first use (default: all-MiniLM-L6-v2, ~22 MB). Both are gated behind `enable_semantic_triage = true`.
- **Storage**: New `.synrepo/index/vectors/` directory (disposable, gitignored). Approximate footprint: ~770 bytes per embedded chunk at 384-dim (float16), ~1540 bytes at 768-dim. A 10k-symbol repo uses ~7.5 MB at default dim.
- **Code**: New module under `src/substrate/` (embedding index respects the layer rule: substrate must not import from structure). Triage integration point in the cross-link generation pipeline.
- **Config**: Four new fields in `src/config.rs`: `enable_semantic_triage`, `semantic_model`, `embedding_dim`, `semantic_similarity_threshold`. Compatibility-sensitive (triggers advisory, not rebuild). Changing `semantic_model` or `embedding_dim` invalidates the vector index.
- **Build time**: ONNX Runtime native library linked only when feature flag `semantic-triage` is enabled. Default build is unaffected.
