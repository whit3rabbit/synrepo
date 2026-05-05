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
| `semantic_model` | `"all-MiniLM-L6-v2"` | Built-in embedding model identifier |
| `embedding_dim` | `384` | Expected embedding dimension for the configured semantic model |
| `semantic_similarity_threshold` | `0.6` | Minimum semantic score for semantic routing and triage matches |
| `[explain].commentary_concurrency` | `4` | Concurrent commentary provider calls during refresh; clamped to at least `1` |
| `auto_sync_enabled` | `true` | Run cheap repair surfaces (export regeneration, retired-observation compaction) automatically after every drift-producing reconcile while watch is active |

## Notes

- Adding a fourth `concept_directories` entry (e.g. `architecture/decisions`) triggers a config-sensitive compatibility check — the compat report raises a graph advisory.
- `include_worktrees` is on by default. Each linked worktree is indexed as a separate root with its own file identity domain.
- `include_submodules` is off by default. When enabled, initialized submodules are indexed as separate roots; nested submodules recurse to a bounded depth.
- `max_graph_snapshot_bytes` is advisory. Oversized snapshots still publish with a warning; set to `0` to force readers onto the SQLite path.
- `redact_globs` is hard: matched files are never indexed and never reach any parser, so they cannot leak into cards, exports, or overlay candidates.
- `auto_sync_enabled` is read once at watch startup and seeds an in-process atomic flag. The dashboard `A` keybinding flips that atomic for the running watch service but does NOT rewrite this file. To change the default persistently, edit `config.toml` and restart watch. The runtime allow-list is hard-coded (`CHEAP_AUTO_SYNC_SURFACES` in `src/pipeline/repair/sync/mod.rs`); commentary refresh and other token-cost surfaces are never auto-run.
- Semantic query paths never download model artifacts. `synrepo init` / `synrepo reconcile` may build vectors when semantic triage is enabled, but `synrepo_task_route` and `synrepo_search` use semantic behavior only when the vector index and model are already local.
- Explain config (`[explain]`) lives in the same file; see `docs/EXPLAIN.md`.
