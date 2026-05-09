# Config fields

Runtime config lives in `.synrepo/config.toml`; the struct is `Config` in `src/config/mod.rs`.

| Field | Default | Notes |
|-------|---------|-------|
| `mode` | `auto` | `auto` or `curated` |
| `roots` | `["."]` | Roots to index, relative to repo root |
| `include_worktrees` | `true` | Include linked git worktrees as additional discovery roots |
| `include_submodules` | `false` | Include initialized git submodules as additional discovery roots |
| `concept_directories` | `["docs/concepts", "docs/adr", "docs/decisions"]` | Concept/ADR dirs; changing this field triggers a graph advisory in the compat report |
| `git_commit_depth` | `500` | History depth budget for deterministic Git-intelligence sampling and file-scoped summaries |
| `max_file_size_bytes` | `1048576` (1 MB) | Files larger than this are skipped |
| `max_graph_snapshot_bytes` | `134217728` (128 MiB) | Advisory ceiling for the published in-memory graph snapshot. `0` disables publication |
| `redact_globs` | `["**/secrets/**", "**/*.env*", "**/*-private.md"]` | Files matching these are never indexed |
| `retain_retired_revisions` | `10` | Compile revisions to keep retired observations before compaction deletes them |
| `enable_semantic_triage` | `false` | Enables local embedding-based triage, semantic routing, and hybrid search when the binary is built with `semantic-triage` and local model assets are available |
| `semantic_embedding_provider` | `"onnx"` | Embedding backend: `onnx` for built-in ONNX Runtime models, or `ollama` for a local Ollama `/api/embed` endpoint |
| `semantic_model` | `"all-MiniLM-L6-v2"` | Built-in ONNX model identifier, or Ollama model name such as `all-minilm` |
| `embedding_dim` | `384` | Expected embedding dimension for the configured semantic model |
| `semantic_similarity_threshold` | `0.6` | Minimum semantic score for semantic routing and triage matches |
| `semantic_ollama_endpoint` | `"http://localhost:11434"` | Base URL for local Ollama embeddings when `semantic_embedding_provider = "ollama"` |
| `semantic_embedding_batch_size` | `128` | Number of texts sent per embedding request during vector index builds |
| `[explain].commentary_concurrency` | `4` | Concurrent commentary provider calls during refresh; clamped to at least `1` |
| `auto_sync_enabled` | `true` | Run cheap repair surfaces (export regeneration, retired-observation compaction) automatically after every drift-producing reconcile while watch is active |

## Notes

- Adding a fourth `concept_directories` entry (e.g. `architecture/decisions`) triggers a config-sensitive compatibility check — the compat report raises a graph advisory.
- `include_worktrees` is on by default. Each linked worktree is indexed as a separate root with its own file identity domain. The dashboard Actions tab exposes a persistent `W` toggle for this field; run `synrepo reconcile` or press `R` afterward to refresh discovered roots.
- `include_submodules` is off by default. When enabled, initialized submodules are indexed as separate roots; nested submodules recurse to a bounded depth.
- `max_graph_snapshot_bytes` is advisory. Oversized snapshots still publish with a warning; set to `0` to force readers onto the SQLite path.
- `redact_globs` is hard: matched files are never indexed and never reach any parser, so they cannot leak into cards, exports, or overlay candidates.
- `auto_sync_enabled` is read once at watch startup and seeds an in-process atomic flag. The dashboard `A` keybinding flips that atomic for the running watch service but does NOT rewrite this file. To change the default persistently, edit `config.toml` and restart watch. The runtime allow-list is hard-coded (`CHEAP_AUTO_SYNC_SURFACES` in `src/pipeline/repair/sync/mod.rs`); commentary refresh and other token-cost surfaces are never auto-run.
- Embeddings are optional and disabled by default. See `docs/EMBEDDINGS.md` for provider setup, dashboard toggling, model choices, and benchmark interpretation.
- Semantic query paths never download model artifacts. `synrepo embeddings build` is the explicit vector-build surface; `synrepo_task_route` and `synrepo_search` use semantic behavior only when the vector index and configured local backend are available.
- ONNX supports the built-in registry only: `all-MiniLM-L6-v2` (384d), `all-MiniLM-L12-v2` (384d), and `all-mpnet-base-v2` (768d). Those ONNX and tokenizer artifacts are fetched from Hugging Face only during `synrepo embeddings build` when embeddings are enabled.
- Ollama embeddings are local-only. With `semantic_embedding_provider = "ollama"`, `semantic_model = "all-minilm"`, and `embedding_dim = 384`, smoke test the endpoint with `curl http://localhost:11434/api/embed -d '{"model":"all-minilm","input":["First sentence","Second sentence"]}'`.
- Explain config (`[explain]`) lives in the same file; see `docs/EXPLAIN.md`.
