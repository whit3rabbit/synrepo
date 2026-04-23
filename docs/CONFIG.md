# Config fields

Runtime config lives in `.synrepo/config.toml`; the struct is `Config` in `src/config/mod.rs`.

| Field | Default | Notes |
|-------|---------|-------|
| `mode` | `auto` | `auto` or `curated` |
| `roots` | `["."]` | Roots to index, relative to repo root |
| `concept_directories` | `["docs/concepts", "docs/adr", "docs/decisions"]` | Concept/ADR dirs; changing this field triggers a graph advisory in the compat report |
| `git_commit_depth` | `500` | History depth budget for deterministic Git-intelligence sampling and file-scoped summaries |
| `max_file_size_bytes` | `1048576` (1 MB) | Files larger than this are skipped |
| `max_graph_snapshot_bytes` | `134217728` (128 MiB) | Advisory ceiling for the published in-memory graph snapshot. `0` disables publication |
| `redact_globs` | `["**/secrets/**", "**/*.env*", "**/*-private.md"]` | Files matching these are never indexed |
| `retain_retired_revisions` | `10` | Compile revisions to keep retired observations before compaction deletes them |

## Notes

- Adding a fourth `concept_directories` entry (e.g. `architecture/decisions`) triggers a config-sensitive compatibility check — the compat report raises a graph advisory.
- `max_graph_snapshot_bytes` is advisory. Oversized snapshots still publish with a warning; set to `0` to force readers onto the SQLite path.
- `redact_globs` is hard: matched files are never indexed and never reach any parser, so they cannot leak into cards, exports, or overlay candidates.
- Explain config (`[explain]`) lives in the same file; see `docs/EXPLAIN.md`.
