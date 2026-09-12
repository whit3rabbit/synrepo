# Changelog

All notable changes to synrepo are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

New version sections are appended automatically when a release tag is pushed
(see the `update-changelog` job in `.github/workflows/release.yml`). To curate
notes by hand, add the `## [x.y.z]` section before tagging; the workflow leaves
an existing section untouched.

## [Unreleased]

## [0.2.1] - 2026-09-12

### Fixed
- CI and release builds now enable `--all-features` (including `semantic-triage` and `metrics-http`) across Linux, Windows, and macOS (Apple Silicon).

## [0.2.0] - 2026-09-12

### Changed
- Upgraded `tree-sitter` to 0.27.0 with adaptation to the private `QueryMatch::captures` getter method.
- Upgraded `gix` to 0.87.1.
- Upgraded `rmcp` to 3.2.0, `interprocess` to 2.4.4, `tokenizers` to 0.23.2, and `sentry` to 0.49.2.
- Cleaned up deprecated Sentry client initialization options (`enable_logs`, `enable_metrics`).
- Scoped `--all-features` in macOS release matrix to Apple Silicon (`aarch64-apple-darwin`), allowing Intel macOS release builds to succeed without unavailable `ort-sys` prebuilts.

### Fixed
- Fixed cascading file-watcher reconcile loops on Linux by ignoring non-mutating `EventKind::Access` events emitted by inotify during compiler AST extraction.
- Hardened watcher path filtering to robustly ignore internal runtime directory paths (`.synrepo`, `.syntext`, `.git`) regardless of relative path formatting.

## [0.1.6] - 2026-09-12

### Added
- Upgraded to `syntext 2.4.0`, adopting `syntext::changes::Catalogue` and
  `Index::apply_change_batch` for fingerprint-based incremental change tracking
  without redundant file re-reads.
- Durable lexical overlay flush: `Index::flush_overlay()` commits in-memory
  changes to disk across restarts without depending on compaction thresholds;
  catalogue generations are acknowledged only when the durable flush succeeds.
- Typed `IndexError::LockConflict` handling with bounded exponential backoff
  (`[20, 40, 80, 160, 200] ms`) on both incremental sync and search open,
  eliminating stringly-typed matching and index lock directory removal.
- Scoped `Index` handle lifetimes: handles and directory locks are dropped
  before entering fallback full rebuilds, preventing self-deadlock.
- Watcher requeue on failure: `PendingWatchChanges::requeue_failed` preserves
  dirty paths and full-reconcile triggers on transient lock conflicts or errors.
- Process-global embedding session cache (`Arc<EmbeddingSession>`) with LRU
  eviction and 30-minute idle TTL, avoiding repeated tokenizer and ONNX model
  loads across task-route classification, hybrid/dense query, and explain triage.
- Incremental embedding index refresh for watch and background auto-refresh:
  unchanged chunks reuse their vectors from the existing index using a
  `(ChunkId, blake3(text))` partition without any schema bump. When no chunks
  change, refresh is a zero-inference no-op that leaves `index.bin` untouched.
  Neural inference runs only for new or modified chunks.
- Process-global graph snapshot registry bounds with 30-minute idle TTL, 32-repo
  cap, and 1 GiB aggregate memory ceiling; structural compile skips snapshot
  publication when source files are unchanged, and the watch service forgets its
  snapshot on teardown.
- MCP blocking tool concurrency semaphore bounding concurrent worker threads to 8
  permits; callers receive `BUSY` on saturation, and timed-out worker tasks hold
  their permit until worker exit to avoid stacking background load.
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
  lexical fallback when no vector hits exist.

### Changed
- Bumped `syntext` dependency to 2.4.0.
- CI release workflow builds macOS and Homebrew binaries with `--all-features`
  (enabling `semantic-triage` embeddings and `metrics-http` by default).
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
- Graph in-memory edge queries (`outbound()`, `inbound()`) filter on borrowed
  edge slices before cloning, reducing heap allocations during graph traversal.

### Fixed
- Watcher suppression refinement: `SuppressedPaths::paths_overlap` no longer
  matches parent directories when a child file is edited, preventing unrelated
  sibling files from being inadvertently suppressed during atomic-write windows.
- Path-seeded file identities: `derive_file_id` includes normalized path alongside
  root discriminant and content hash, ensuring byte-identical files in the same
  root receive distinct graph identities and independent lifecycles.
- Stale snapshot eviction: if a recompiled graph exceeds the configured snapshot
  memory ceiling, `snapshot::forget(repo_root)` removes the existing snapshot
  from the registry to avoid serving stale in-memory state.
- Stage 4 callee prefix scoring checks candidate file/module stems against callee
  prefixes for accurate cross-file call resolution.

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
