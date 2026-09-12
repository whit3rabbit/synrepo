# Optional Embeddings

Embeddings are optional in synrepo. They are a recall aid for semantic routing and hybrid search, not a replacement for lexical search, graph cards, or deterministic task routing.

Use them when your own benchmark tasks show fewer misses. Leave them off when exact symbol, path, or string lookup is already enough.

## Build

To use embeddings, you must build the binary with the `semantic-triage` feature enabled:

```bash
cargo build --features semantic-triage
# or for a release build
cargo build --release --features semantic-triage
```

If installing via Cargo, use:

```bash
cargo install --path . --features semantic-triage
```

This pulls in the `ort` (ONNX Runtime), `tokenizers`, and `ndarray` dependencies.

Official Homebrew and macOS release binaries are built with all features (`--all-features`), including `semantic-triage` (embeddings) and `metrics-http`. For custom or base builds, use a source or Cargo install with `--features semantic-triage`.

## Gates

Embeddings only participate when all gates are open:

1. The binary is built with `semantic-triage` (see [Build](#build)).
2. `.synrepo/config.toml` has `enable_semantic_triage = true`.
3. `synrepo embeddings build` has built `.synrepo/index/vectors/<key>-<label>/index.bin` for the active profile.
4. Query-time code can load the vector index and local embedding backend.

If any gate is closed, MCP and CLI search fall back to lexical behavior with `semantic_available: false` or `routing_strategy: "keyword_fallback"`. Query-time surfaces do not download ONNX artifacts, rebuild indexes, or start background work.

After the first explicit build, `synrepo watch` can refresh the existing vector index in the background when `auto_sync_enabled = true`. A successful non-keepalive reconcile with touched source paths, or a path-overflow full reconcile, marks the existing index stale. Watch waits for a 30 second quiet window with no pending filesystem changes, no running sync, and no running embedding job before refreshing. This refresh is conservative and incremental: it does not create the first index, does not download ONNX artifacts, and uses only cached ONNX assets or the configured local Ollama endpoint. Unchanged chunks reuse their vectors from the existing profile index via `(ChunkId, blake3(text))` partitioning; when all chunks are unchanged, refresh is a zero-inference no-op that leaves `index.bin` untouched. Provider preflight and embedding batches run only for new or modified chunks. Explicit `synrepo embeddings build` remains a deliberate full rebuild. Failed background refreshes back off before retrying.

Installed Git hooks stay cheap: they run `synrepo reconcile --fast`, not an embedding build. When watch is active, that delegated reconcile marks an existing vector index stale and lets the same watch-owned quiet-window refresh path handle it. Without watch, hooks refresh graph and lexical state only; rebuild vectors explicitly with `synrepo embeddings build` when semantic freshness matters.

## TUI Management

In the dashboard Actions tab, press `T` to enable or disable embeddings for the current repo. The action writes only `enable_semantic_triage` in `.synrepo/config.toml`.

After enabling, press `B` in the dashboard or run `synrepo embeddings build` to build vectors. The builder shows progress, preflights the provider, and reports failures such as an unavailable Ollama endpoint or wrong vector dimension. If watch is running, explicit builds delegate to the watch service so the daemon remains the write authority.

If the binary was not built with `semantic-triage`, enabling from the TUI reports that embeddings are unavailable. Disabling remains allowed.

## Providers

### ONNX

ONNX is the default provider:

```toml
enable_semantic_triage = true
semantic_embedding_provider = "onnx"
semantic_model = "snowflake-arctic-embed-xs"
embedding_dim = 384
```

Supported built-in models:

| Model | Source | Dimension | Notes |
|-------|--------|-----------|-------|
| `snowflake-arctic-embed-xs` | Hugging Face ONNX artifact | 384 | Default. CLS pooling. Requires the standard instruction prefix `"Represent this sentence for searching relevant passages: "` on the query side; documents are embedded without the prefix. Measured +7.1 pp recall over MiniLM on the blind benchmark. |
| `all-MiniLM-L6-v2` | Hugging Face ONNX artifact | 384 | Alternative. Fastest built-in. Mean pooling. No query prefix. 10 MB smaller download than arctic-xs. |
| `all-MiniLM-L12-v2` | Hugging Face ONNX artifact | 384 | Larger MiniLM variant. Mean pooling. No query prefix. |
| `all-mpnet-base-v2` | Hugging Face ONNX artifact | 768 | Higher-dimensional, slower and larger. Mean pooling. No query prefix. |

The built-in registry downloads `model.onnx` and `tokenizer.json` during `synrepo embeddings build` only when embeddings are enabled. Arbitrary Hugging Face repo IDs are not accepted yet because pooling, tokenizer shape, normalization, query prefix behavior, and dimensions need an explicit registry entry.

### Ollama

Ollama is local-only and uses `/api/embed`:

```toml
enable_semantic_triage = true
semantic_embedding_provider = "ollama"
semantic_model = "all-minilm"
embedding_dim = 384
semantic_ollama_endpoint = "http://localhost:11434"
semantic_embedding_batch_size = 128
```

Smoke test:

```bash
curl http://localhost:11434/api/embed -d '{"model":"all-minilm","input":["First sentence","Second sentence"]}'
```

Expected result: two embeddings, each 384 dimensions for `all-minilm`. Synrepo validates response count and dimension, then normalizes vectors before persisting the index.

## Storage layout and profile keys

The vector index is built and read by
`src/substrate/embedding/index/persistence.rs`. Each index lives under
`.synrepo/index/vectors/<key>-<label>/index.bin`, where `<key>` is the
first 16 hex chars of `blake3` over the canonical key string
`<provider>-<model>-d<dim>-p<precision>-c<chunk_chars>-n<normalizer_version>`
(for example `onnx-all-MiniLM-L6-v2-d384-float32-c512-n1`; see
`VectorProfile::key_string` in `src/substrate/embedding/profile.rs`) and
`<label>` is a human-readable short form like
`onnx-all-MiniLM-L6-v2-d384-float32`. The label makes the profile
recoverable from `ls .synrepo/index/vectors/` without recomputing the
key.

Changing any field in the profile tuple — including `semantic_model`,
`embedding_dim`, or `semantic_vector_precision` — produces a different
profile key. The OLD index remains readable at its old path until
`synrepo embeddings clean --apply` removes it; the NEW index is built
side by side. This
means model swaps do not silently invalidate the previous index; the new
profile is built next to it and the loader picks the one that matches
the active config.

`synrepo embeddings clean` lists removable artifacts: profile-shaped
subdirectories that no longer match the active config, plus the legacy
flat `index.bin` from pre-v6 layouts. It is a dry run by default;
`--apply` deletes (serialized against watch reconciles through the
writer lock), and `--json` emits a machine-readable summary. The active
profile's directory is never a candidate, even when embeddings are
disabled; unrecognized non-profile entries under the vectors root are
never touched.

The on-disk format version is `INDEX_FORMAT_VERSION = 6` (in
`src/substrate/embedding/index/persistence.rs`). v5 and earlier refuse to
load (fail-closed on unknown version). The full v6 header layout is in
[`docs/SCHEMA.md` § Embedding index](./SCHEMA.md#embedding-index-synrepoindexvectorsprofileindexbin).

## Precision: float32 (default) vs int8 (gated)

`semantic_vector_precision = "float32" | "int8"` selects the on-disk
quantization for the active profile. The default is `"float32"`.
`"int8"` is gated by the [Vector Compression Gate](#vector-compression-gate)
below and is not the recommended default — see that section for the
bench gate before flipping it on.

In-memory scoring stays on `Vec<f32>` regardless of the on-disk
precision. int8 is a storage concern only: each vector is per-vector
symmetrically quantized to `[-127, 127]` using its own `max_abs` as
scale, plus a single `f32` scale per vector (so the float32 vector can
be reconstructed at load time). The zero-vector case is handled by
checking `max_abs == 0` before dividing.

Query-time embedding uses the same `float32` model output; the
quantization only happens during `save()`. Tests in
`src/substrate/embedding/index/persistence.rs` exercise the
float32 ↔ int8 round-trip and verify the on-disk size shrinks.

## Evaluation

Run a lexical baseline and hybrid comparison against task fixtures:

```bash
cargo run -- bench context --tasks 'benches/tasks/*.json'
cargo run --features semantic-triage -- bench search --tasks 'benches/tasks/*.json' --mode both --json
# To compare dense-first against auto (RRF), use --mode all instead:
cargo run --features semantic-triage -- bench search --tasks 'benches/tasks/*.json' --mode all --json
```

The bench `--mode` flag accepts:

- `lexical` — run only the lexical arm
- `auto` — run only the auto (RRF) arm
- `dense-first` — run only the dense-first arm (cosine-ranked vectors; lexical fallback when no vector hits)
- `both` — run `lexical` + `auto` (default for comparison against the June-2026 baseline)
- `all` — run `lexical` + `auto` + `dense-first` (needed to read off the `dense_first_vs_rrf_*` summary fields)

Read the benchmark as a tradeoff:

- `hit@5`: whether expected targets appeared in the top five results.
- `semantic_available_tasks`: how often the vector path was actually usable.
- `hybrid_improved_tasks`: tasks where auto search found a target lexical missed.
- `hybrid_regressed_tasks`: tasks where auto search lost a lexical hit.
- `dense_first_vs_rrf_wins`: tasks where dense-first hit and auto missed.
- `dense_first_vs_rrf_regressions`: tasks where dense-first missed and auto hit.
- `latency_ms`: hybrid should be expected to cost more than lexical.

Observed local baseline on this repo, recorded 2026-06-05:

Command:

```bash
cargo run --features semantic-triage -- bench search --tasks 'benches/tasks/*.json' --mode both --json
```

Vector index size:

```bash
du -sh .synrepo/index/vectors
# 15M .synrepo/index/vectors   (float32, current default)
# The actual subdirectory is named `<blake3-prefix>-<label>` where
# `<label>` is e.g. `onnx-all-MiniLM-L6-v2-d384-float32`. Inspect with
# `ls .synrepo/index/vectors/` to recover the active profile; the
# label makes the model, dim, and precision readable without recomputing
# the key. A second profile (e.g. int8 or arctic-xs) lives in a sibling
# subdirectory until `synrepo embeddings clean --apply` removes unused profiles.
```

| Mode | hit@5 | Total latency | Notes |
|------|------:|--------------:|-------|
| lexical | 0.571 | 312 ms | 14 checked-in tasks |
| auto hybrid | 0.929 | 8590 ms | semantic available for all 14 tasks |
| dense-first (vector-only, lex fallback) | 0.857 | n/a | regresses 1 task vs RRF on this corpus |

Hybrid improved 5 tasks, matched 9, and regressed 0. That result says
embeddings are useful for recall on these broad benchmark tasks, not that they
should be enabled everywhere. For exact symbol names, paths, flags, or error
strings, lexical search is still the faster and more predictable route.

### Dense-first vs auto (RRF)

A `dense-first` ranking arm is also wired in (`synrepo bench search --mode all`)
for measurement. It runs the vector lane alone ranked by cosine, falling
back to lexical only when the vector index is missing or returns zero hits.
On the same 14 in-house tasks, with the syntext segment stable across runs
(no reconcile between bench invocations):

| Arm | hit@5 | total latency |
|---|---:|---:|
| lexical | 0.571 | 148 ms |
| auto (RRF) | 0.929 | 4170 ms |
| dense-first | 0.857 | 3235 ms |
| dense-first vs auto | **0 wins, 1 regression** | -22% latency |

The single regressed task is `impact_or_risk/command-execution-review`
(query `Command::new`). 36 places in the repo use `Command::new`, cosine
similarity ties across all of them, and the lexical top hits also do not
include the right file. **The RRF fusion lifts the correct symbol into
the top 5 by combining lexical and semantic contributions even though
neither lane alone surfaces it cleanly.** Dense-first ranking drops that
fusion and the symbol falls off.

Why this inverts `memoryfield`'s finding: their blind query set had
queries with vocabulary that diverged sharply from the corpus, so
cosine was the only useful signal and lexical was pure noise. synrepo's
in-house fixture queries are identifier-style (`fn atomic_write`,
`hold_writer_flock_with_ownership`, etc.), and on those cosine ties
frequently because the symbol name dominates the chunk's prose either
way. RRF's averaging behavior rescues cosine ties from lexical priors
that point at the same file.

> **Caveat:** the bench's lexical arm is non-deterministic across syntext
> state transitions (segment merges after reconcile, file additions).
> Per-task hit counts can drift by ±1 between runs that span a reconcile.
> The dense-first vs auto *comparison* is still valid because both arms
> see the same lexical state on the same run.

Conclusion: **do not switch `synrepo_search mode=auto` to dense-first on
this corpus.** The 22% latency saving isn't worth a 7pp hit@5 regression
on the bench. Dense-first may still win on a blind query set or against
unfamiliar repos; that's a separate bench that needs authoring work.

## Default model: snowflake-arctic-embed-xs
 
`snowflake-arctic-embed-xs` is the default ONNX embedding model for synrepo. The decision to make arctic-xs the default is bench-driven by the 14-task blind benchmark suite (`benches/tasks_blind/*.json`):

- **In-vocabulary tasks (`benches/tasks/*.json`):** On the 14 in-house tasks with exact code identifiers, both `all-MiniLM-L6-v2` and `snowflake-arctic-embed-xs` hit 0.929 hit@5 identically, with arctic-xs showing a ~1.8% latency advantage.
- **Blind stress test (`benches/tasks_blind/*.json`):** When queries are paraphrased into natural language without code identifiers (driving lexical hit@5 to 0.000), arctic-xs achieves **0.429 (6/14) hit@5 vs MiniLM's 0.357 (5/14)** (+7.1 pp), recovering `src/util/atomic_write.rs` from natural descriptions that MiniLM missed entirely, with zero regressions and 5% lower latency.

### Blind benchmark set (`benches/tasks_blind/*.json`)

To test model quality without in-vocabulary lexical contamination, the repository maintains a 14-task blind benchmark suite under `benches/tasks_blind/*.json`. Every query is paraphrased into natural English without mentioning the target symbol name, signature, or filename (driving lexical hit@5 to 0.000).

Reproduce:

```bash
cargo run --features semantic-triage -- bench search --tasks 'benches/tasks_blind/*.json' --mode all --json
```

Measured outcome on the 14 blind tasks:

| Metric | `snowflake-arctic-embed-xs` (Default) | `all-MiniLM-L6-v2` | Δ |
|---|:---:|:---:|:---:|
| `lexical hit@5` | 0.000 (0/14) | 0.000 (0/14) | 0 |
| `auto hit@5` (hybrid RRF) | **0.429 (6/14)** | 0.357 (5/14) | **+1 hit (+7.1 pp)** |
| `dense-first hit@5` | **0.429 (6/14)** | 0.357 (5/14) | **+1 hit (+7.1 pp)** |
| `hit@1` (top-1 accuracy) | 0.143 (2/14) | **0.214 (3/14)** | -1 hit (-7.1 pp) |
| `hybrid_improved_tasks` | 6 | 5 | +1 |
| `auto_latency_ms` | 4598 ms | 4840 ms | **-242 ms (-5.0%)** |

Key blind findings:
- Arctic-xs breaks the tie on pure semantic search: it recovers `src/util/atomic_write.rs` from `"safely replace destination file using temporary staging and rename to prevent corruption"`, which MiniLM completely misses.
- Zero regressions: Arctic-xs hits all 5 tasks MiniLM hits, plus 1 additional task.
- MiniLM maintains slightly higher top-1 precision on its hits (e.g. ranking writer flock helper #1 vs #4 for Arctic).
- Switching to MiniLM remains supported via `semantic_model = "all-MiniLM-L6-v2"` in `.synrepo/config.toml` for users prioritizing a smaller on-disk model artifact (87 MB vs 97 MB).

## int8 precision status

`semantic_vector_precision = "int8"` is implemented (v6 binary header,
per-vector symmetric quantization with `max_abs` scale, `Vec<f32>` in
memory) but **the default is `float32` and int8 is not flagship-ready.**
The compression gate above lists the bench criteria that must hold
before int8 is shipped as the default precision. This PR did not run
the int8 bench; the gate is intentionally a follow-up PR rather than
auto-promoted alongside the toggle.

## Vector Compression Gate

Binary or quantized vector storage is only worth landing if a feature-branch
benchmark proves all of these against the baseline above:

- `.synrepo/index/vectors/<key>-<label>/` is materially smaller than 14 MB for this repo when `semantic_vector_precision = "int8"` (the gate's reference point; `float32` is the default and produces the ~15 MB index reported in the table above).
- `auto` hit@5 does not fall below 0.929 on `benches/tasks/*.json`.
- `hybrid_regressed_tasks` stays at 0.
- Total auto latency does not increase materially from 4170 ms.
- The index remains disposable, rebuildable, and gated by `semantic-triage`.
