# Changelog

All notable changes to synrepo are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

New version sections are appended automatically when a release tag is pushed
(see the `update-changelog` job in `.github/workflows/release.yml`). To curate
notes by hand, add the `## [x.y.z]` section before tagging; the workflow leaves
an existing section untouched.

## [Unreleased]

### Added
- `.synrepoignore` filter file recognized alongside `.synignore` for both
  file discovery (`src/substrate/discover.rs`) and the watch filter
  (`src/pipeline/watch/filter.rs`). Additive with `.synignore`; use
  `!path` negation to override a synignore match.
- `synrepo embeddings clean` lists and removes vector-index artifacts that
  no longer match the active config profile: stale profile subdirectories
  and the legacy flat v5 `index.bin`. Dry run by default, `--apply` to
  delete, `--json` for machine-readable output. The active profile and
  non-profile entries under the vectors root are never touched.
- `snowflake-arctic-embed-xs` registered in the ONNX model registry
  (`src/substrate/embedding/model/resolution.rs`). Opt-in. Uses CLS pooling
  with the standard Snowflake instruction prefix
  `"Represent this sentence for searching relevant passages: "` applied at
  query time only (documents are embedded without the prefix). See
  `docs/EMBEDDINGS.md` § "Model swap: arctic-xs vs MiniLM" for measured
  hit@5 against the in-house bench.
- `EmbeddingSession::embed_query(text)` separate from `embed(texts)`, so
  instruction-tuned models can apply a query prefix without leaking it to
  document embeddings. `FlatVecIndex::embed_text` routes through
  `embed_query`, so all query-time call sites (hybrid search, cross-link
  triage, semantic-triage task route) pick up the prefix automatically.
- `semantic_vector_precision` config field
  (`docs/CONFIG.md`). Selects the on-disk quantization for the active
  vector index profile; values are `"float32"` (default) or `"int8"`.
  int8 is gated by `docs/EMBEDDINGS.md` § "Vector Compression Gate".
- `dense-first` ranking arm on `synrepo bench search`
  (`--mode dense-first` and `--mode all`). Cosine-ranked vector hits with
  lexical fallback when no vector hits exist. New per-task and per-summary
  fields: `dense_first_hit_at_5`, `dense_first_latency_ms`,
  `dense_first_vs_rrf_wins`, `dense_first_vs_rrf_regressions`. Measured
  on the in-house bench: 0 wins, 1 regression vs auto RRF
  (`docs/EMBEDDINGS.md` § "Dense-first vs auto (RRF)"). Not yet wired
  into `synrepo_search mode=auto`.

### Changed
- Vector index storage layout moved from a flat
  `.synrepo/index/vectors/index.bin` (v5) to profile-keyed
  `.synrepo/index/vectors/<blake3-prefix>-<label>/index.bin` (v6).
  Changing `semantic_model`, `embedding_dim`, or `semantic_vector_precision`
  builds a new profile side by side with the old one rather than
  invalidating it silently; `synrepo embeddings clean` lists and removes
  unused profiles (dry run by default, `--apply` to delete).
- `INDEX_FORMAT_VERSION` bumped 5 → 6
  (`src/substrate/embedding/index/persistence.rs`). v5 and earlier refuse
  to load (fail-closed). v6 header layout is documented in `docs/SCHEMA.md`
  § "Embedding index".
- Default model recommendation (`docs/EMBEDDINGS.md`) keeps
  `all-MiniLM-L6-v2` as the recommended default — `snowflake-arctic-embed-xs`
  is opt-in because its MF-measured advantage on blind prose benchmarks
  does not transfer to the in-house code-search fixture set
  (`docs/EMBEDDINGS.md` § "Model swap: arctic-xs vs MiniLM").
- Bumped `syntext` dependency from 2.0.0 to 2.3.0. No source changes
  were required: `Index::search`, `Index::build_from_file_records`,
  `SearchOptions`, and `IndexError::LockConflict` are stable across
  2.0.0 → 2.3.0. Inherited fixes: `ENOLCK`/`EINTR` flock failures now
  surface as retryable `LockConflict` (2.1.0), and the substring-based
  `is_lock_conflict` check in `src/substrate/index.rs` continues to
  match the stable "index locked by another process" wording. New
  `Index::search_fresh` / `Index::update_from_git` are not adopted
  in this bump: synrepo's watch daemon and `substrate::incremental`
  already drive bounded refresh on its own path set, and `git diff`-
  driven change detection in `git_intelligence` runs against its own
  `--name-only` invocation; adding a second git detector would create
  the staleness inconsistencies syntext 2.3.0 just spent its changelog
  closing. Follow-up worth considering: replace the stringly-typed
  `is_lock_conflict` substring check with a direct match on the
  `IndexError::LockConflict(_)` variant now that the variant is the
  single lock-error surface upstream.

## [0.1.5] - 2026-08-15

### Fixed
- Allow safe in-repo symlinks (such as `.agents -> .claude` or `CLAUDE.md -> AGENTS.md`) during file discovery, runtime probing, and agent shim installation while maintaining strict rejection of out-of-repo symlink targets.
- Fix removal planning and deletion to properly detect and clean up dangling or broken symlinks.

## [0.1.4] - 2026-08-13

### Changed
- Bump agent-config, rmcp, sentry, and syntext to latest major versions

## [0.1.3] - 2026-06-14

### Fixed
- Watch mode now disables notify-debouncer file-ID caching so macOS does not
  recursively scan large ignored build trees before synrepo can filter events.
- Watch event filtering now honors repo-root `.gitignore`, `.git/info/exclude`,
  and `.synignore` matches before queuing reconcile work, preventing ignored
  Cargo `target/` churn from waking the daemon.

## [0.1.2] - 2026-06-13

### Added
- Client-side agent hooks emit advisory read-cost and repeated-read hints from
  bounded per-repo state (`.synrepo/state/agent-hook-reads.json`: relative path,
  size, mtime, timestamps, read count, estimated tokens; 8h TTL, 256-entry cap).
  Stored as metadata only, never file contents.
- `hook_file_reads_total` / `hook_repeated_read_*` context metrics count read
  observations without storing paths or content.
- MCP final response clamp compacts over-budget JSON before destructive
  truncation: search-shaped payloads reuse the compact search representation,
  other known row arrays keep routing identifiers and bounded string previews
  (`response_omitted[].strategy = "row_compaction"`).
- Opt-in Sentry telemetry with a built-in fallback DSN, following the existing
  sanitized failed-tool privacy boundary.

### Changed
- Direct-reader Bash parsing never treats `sed -i` / `--in-place` edits as reads.
- Compact-search parallel arrays (`file_groups`, `suggested_card_targets`,
  `suggested_card_requests`) are realigned after trimming so they stay 1:1.

## [0.1.1] - 2026-06-06

### Added
- Opt-in MCP telemetry controls.

### Changed
- Pinned release tooling and added backoff for explain retries.
- Bumped the Rust dependency group.

## [0.1.0] - 2026-06-05

- First tagged 0.1 release. See Git history (`git log v0.0.11..v0.1.0`) for
  the full set of changes from the 0.0.x series.

[Unreleased]: https://github.com/whit3rabbit/synrepo/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/whit3rabbit/synrepo/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/whit3rabbit/synrepo/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/whit3rabbit/synrepo/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/whit3rabbit/synrepo/releases/tag/v0.1.0
