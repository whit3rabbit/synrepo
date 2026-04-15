## Context

Cross-link candidate generation in `cross-link-overlay-v1` uses a two-stage pipeline: (1) deterministic prefilter (name/identifier token overlap + graph distance cutoff), then (2) LLM evidence extraction on surviving pairs. The deterministic prefilter is fast and bounded but misses candidates where prose describes a concept without sharing lexical tokens with the symbol name (e.g., prose says "connection pooling" but the symbol is `DbPool::acquire`).

ROADMAP.md Track K scopes "bounded semantic linking" and §9.11 explicitly permits embeddings as an opt-in candidate generator. The archived cross-link-overlay-v1 design (D1) listed "embedding similarity prefilter" as a deferred alternative. This change picks up that deferral.

Current state:
- Cross-link candidate generation and overlay storage are wired end-to-end via `cross-link-overlay-v1`.
- `src/substrate/index.rs` wraps the syntext lexical index. Adding an embedding index alongside it respects the layer rule (substrate must not import from structure).
- `enable_semantic_triage` does not exist in `src/config.rs` yet.

## Goals / Non-Goals

**Goals:**
- Add an opt-in embedding similarity prefilter that runs after the deterministic prefilter and before LLM verification, catching pairs the deterministic pass missed.
- Use a local ONNX runtime (`ort` crate) with all-MiniLM-L6-v2 (384-dim, ~80 MB model). No network calls at inference time.
- Keep the vector index disposable: stored under `.synrepo/index/vectors/`, gitignored, rebuildable from the graph alone.
- Gate everything behind `enable_semantic_triage = true` (config) and a Cargo feature flag `semantic-triage` (default off).
- Feed semantic candidates into the exact same LLM evidence extraction and confidence scoring pipeline as deterministic candidates.

**Non-Goals:**
- Vector search as a primary retrieval surface (no MCP tool, no card field).
- Persistent vector store beyond the local index (no remote, no shared state).
- Replacing the deterministic prefilter (it remains the fast path; semantic is supplementary).
- Graph-level embedding edges or similarity scores in the canonical graph.
- Embedding the full file text (only symbol signatures and prose concept chunks are embedded).

## Decisions

### D1: Embedding runs at substrate-layer index time, not at triage time
**Decision**: Embed text chunks when the substrate index is built (during `synrepo init`/`synrepo reconcile`). Store vectors in `.synrepo/index/vectors/`. At triage time, compute cosine similarity against the pre-built index.

**Rationale**: Embedding at query time would mean re-embedding the same text on every candidate generation pass. Index-time embedding amortizes the cost and keeps triage latency low (just a dot product sweep). The index is disposable because it derives entirely from graph content.

**Alternatives considered:**
- Query-time embedding (wasteful; re-embeds unchanged text repeatedly)
- External vector service (violates local-only product thesis; adds network dependency)

### D2: Configurable ONNX model via `ort`, default all-MiniLM-L6-v2
**Decision**: Use the `ort` crate (ONNX Runtime bindings) with dynamic library loading. The model is configurable: a `semantic_model` config field accepts a model name (resolved against a built-in registry) or an absolute path to a local `.onnx` file. Default: `all-MiniLM-L6-v2` (384-dim, ~22 MB). The model file is stored at `.synrepo/index/vectors/model.onnx`, downloaded on first use if not already present. The config also accepts `embedding_dim` to match the model's output dimension (default: 384); a mismatch between `embedding_dim` and the model's actual output is caught at index build time and fails fast with a clear error.

**Rationale**: Different repos benefit from different accuracy/cost tradeoffs. L6 is the right default (smallest download, fastest inference, adequate recall for a prefilter). But users working with large, domain-specific codebases may prefer E5-base or BGE-base (768-dim, higher recall, ~5x model size). Making the model configurable avoids baking a single choice into the spec while keeping the default lightweight. `ort` provides hardware-optimized inference (CPU, GPU if available) with minimal Rust-side code.

**Built-in registry** (model name → download URL + expected dim):
- `all-MiniLM-L6-v2` → 384-dim (default)
- `all-MiniLM-L12-v2` → 384-dim
- `all-mpnet-base-v2` → 768-dim

**Hugging Face model IDs** (e.g., `intfloat/e5-base-v2`, `BAAI/bge-base-en-v1.5`): when `semantic_model` contains a `/`, it is treated as a Hugging Face model ID. The system downloads the ONNX variant from `https://huggingface.co/{model_id}/resolve/main/model.onnx` (or the configured HF mirror). The user must set `embedding_dim` to match the model's output dimension. The downloaded file is cached at `.synrepo/index/vectors/model.onnx` alongside built-in models.

**Local paths**: absolute path ending in `.onnx` is used directly, no download.

Resolution order: (1) built-in registry match, (2) contains `/` → Hugging Face ID, (3) ends in `.onnx` → local path, (4) error.

**Alternatives considered:**
- Hardcode L6 (too rigid; penalizes users who want better recall at higher cost)
- `candle` (more code to maintain; no quantization benefit at this model size)
- `tract` (narrower ONNX op support; higher risk of compatibility issues)
- Hugging Face-only with no built-in registry (forces network on first use even for default; built-in models should be self-describing)

### D3: Two-pass triage: deterministic first, then semantic fill
**Decision**: The deterministic prefilter runs first (unchanged). Its discarded pairs are the input to the semantic prefilter. Semantic candidates that exceed the similarity threshold are added to the LLM verification queue alongside deterministic candidates.

**Rationale**: Deterministic is cheaper (token comparison) and catches the easy cases. Running semantic on only the deterministic-discarded set bounds the cosine similarity work. The LLM verification stage is the same for both sources, so confidence scoring and overlay storage are shared.

**Alternatives considered:**
- Semantic-first (wastes embedding compute on pairs the deterministic prefilter would catch trivially)
- Parallel deterministic + semantic (redundant LLM calls on overlapping pairs; requires dedup)
- Semantic only (loses the cheap fast path; ROADMAP says deterministic remains primary)

### D4: Chunk strategy: symbol qualified_name + signature, prose concept full text
**Decision**: Each symbol produces one embedding vector from `qualified_name + " " + signature`. Each prose concept (from concept directories) produces one embedding from its full text (truncated to 512 tokens). File-level embedding is out of scope.

**Rationale**: Symbol names + signatures are compact and discriminative. Full prose text captures the concept's semantic content. 512-token truncation matches the model's effective context window. File-level embedding would be too coarse to be useful for cross-link pairs.

**Alternatives considered:**
- Embed full function bodies (too expensive; noise from boilerplate)
- Embed doc comments only (misses symbols without docs, which are the ones most likely to benefit from semantic matching)

### D5: FlatVecs with brute-force cosine similarity, no ANN index
**Decision**: Store vectors in a flat array (one `Vec<f16>` per chunk, plus a parallel `Vec<ChunkId>`). At query time, compute cosine similarity against all chunk vectors. No approximate nearest neighbor (HNSW, IVF, etc.).

**Rationale**: A 10k-symbol repo produces ~10k vectors at 384 dims. Brute-force cosine over 10k vectors takes <1 ms. ANN adds complexity (dependency, index build cost, tuning) for no measurable latency benefit at this scale. If repos exceed 100k symbols, revisit.

**Alternatives considered:**
- `hnswlib` or `instant-distance` (unnecessary at current scale; adds dependency)
- Sled/Redb as vector store (overkill; the index is a flat array)

### D6: Feature flag `semantic-triage` gates dependency inclusion
**Decision**: Add a Cargo feature flag `semantic-triage` that gates the `ort` dependency and the embedding module. Default build (`cargo build`) does not include ONNX Runtime. Users opt in via `cargo build --features semantic-triage`.

**Rationale**: ONNX Runtime is a large native dependency (~50 MB library). Users who do not want semantic triage should not pay the build cost or download. The feature flag is separate from the config flag: the feature enables compilation, the config enables runtime behavior.

**Alternatives considered:**
- Always compile, config-gate at runtime (forces all users to install ONNX Runtime)
- Separate crate (overly complex for a single module behind a feature flag)

## Risks / Trade-offs

- **[Model download on first use]** Users with `enable_semantic_triage = true` need to download the model on first init (22 MB for default L6, up to ~110 MB for larger models). Mitigation: clear progress indicator, documented offline path (place `model.onnx` manually or point `semantic_model` at a local path).
- **[Build complexity for ONNX Runtime]** `ort` requires the ONNX Runtime native library. Mitigation: feature flag ensures default builds are unaffected. CI tests default features only; semantic-triage CI is opt-in.
- **[Memory at index build time]** Embedding 10k chunks requires loading the model (~80 MB) and producing vectors. Mitigation: batched inference, memory-bounded by config. The model is loaded once and dropped after index build.
- **[False positives from semantic similarity]** Cosine similarity on short text can produce noisy matches. Mitigation: the LLM verification stage catches false positives before they become candidates. The similarity threshold is configurable.
- **[Vector index staleness]** The index is built at reconcile time. File changes between reconciles are not reflected. Mitigation: same staleness model as the lexical index; `synrepo reconcile` or watch-triggered reconcile rebuilds it.
