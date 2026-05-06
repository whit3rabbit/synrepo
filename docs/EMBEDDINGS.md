# Optional Embeddings

Embeddings are optional in synrepo. They are a recall aid for semantic routing and hybrid search, not a replacement for lexical search, graph cards, or deterministic task routing.

Use them when your own benchmark tasks show fewer misses. Leave them off when exact symbol, path, or string lookup is already enough.

## Gates

Embeddings only participate when all gates are open:

1. The binary is built with `semantic-triage`.
2. `.synrepo/config.toml` has `enable_semantic_triage = true`.
3. `synrepo reconcile` has built `.synrepo/index/vectors/index.bin`.
4. Query-time code can load the vector index and local embedding backend.

If any gate is closed, MCP and CLI search fall back to lexical behavior with `semantic_available: false` or `routing_strategy: "keyword_fallback"`. Query-time surfaces do not download ONNX artifacts, rebuild indexes, or start background work.

## TUI Management

In the dashboard Actions tab, press `T` to enable or disable embeddings for the current repo. The action writes only `enable_semantic_triage` in `.synrepo/config.toml`.

After enabling, run `R` in the dashboard or `synrepo reconcile` to build vectors. The TUI does not start that rebuild automatically because embedding can download local model artifacts for ONNX or call a local Ollama endpoint over many chunks.

If the binary was not built with `semantic-triage`, enabling from the TUI reports that embeddings are unavailable. Disabling remains allowed.

## Providers

### ONNX

ONNX is the default provider:

```toml
enable_semantic_triage = true
semantic_embedding_provider = "onnx"
semantic_model = "all-MiniLM-L6-v2"
embedding_dim = 384
```

Supported built-in models:

| Model | Source | Dimension | Notes |
|-------|--------|-----------|-------|
| `all-MiniLM-L6-v2` | Hugging Face ONNX artifact | 384 | Default, fastest built-in |
| `all-MiniLM-L12-v2` | Hugging Face ONNX artifact | 384 | Larger MiniLM variant |
| `all-mpnet-base-v2` | Hugging Face ONNX artifact | 768 | Higher-dimensional, slower and larger |

The built-in registry downloads `model.onnx` and `tokenizer.json` during `synrepo init` or `synrepo reconcile` only when embeddings are enabled. Arbitrary Hugging Face repo IDs are not accepted yet because pooling, tokenizer shape, normalization, and dimensions need an explicit registry entry.

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

## Evaluation

Run a lexical baseline and hybrid comparison against task fixtures:

```bash
cargo run -- bench context --tasks 'benches/tasks/*.json'
cargo run --features semantic-triage -- bench search --tasks 'benches/tasks/*.json' --mode both --json
```

Read the benchmark as a tradeoff:

- `hit@5`: whether expected targets appeared in the top five results.
- `semantic_available_tasks`: how often the vector path was actually usable.
- `hybrid_improved_tasks`: tasks where auto search found a target lexical missed.
- `hybrid_regressed_tasks`: tasks where auto search lost a lexical hit.
- `latency_ms`: hybrid should be expected to cost more than lexical.

Observed local baseline on this repo with Ollama `all-minilm`:

| Mode | hit@5 | Total latency |
|------|-------|---------------|
| lexical | 0.25 | 49 ms |
| auto hybrid | 1.00 | 690 ms |

That result says embeddings are useful for recall on these four broad tasks, not that they should be enabled everywhere. For exact symbol names, paths, flags, or error strings, lexical search is still the faster and more predictable route.
