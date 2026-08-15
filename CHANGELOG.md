# Changelog

All notable changes to synrepo are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

New version sections are appended automatically when a release tag is pushed
(see the `update-changelog` job in `.github/workflows/release.yml`). To curate
notes by hand, add the `## [x.y.z]` section before tagging; the workflow leaves
an existing section untouched.

## [Unreleased]

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
