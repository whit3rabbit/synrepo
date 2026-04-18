## Context

`GitCache` (`src/surface/card/compiler/git_cache/mod.rs`) memoises per-file git-intelligence projections. The cache lives inside `GraphCardCompiler` and is consulted during card compilation (FileCard `git_intelligence`, SymbolCard `last_change`, etc.). Key design points:

- Keyed by `String` (repo-relative path).
- `BoundedPathCache` with 32,768-entry FIFO eviction (`PATH_CACHE_CAPACITY`).
- A `RwLock<Inner>` wraps a three-state machine: `Uninitialized`, `Unavailable`, `Ready { index, head_sha, paths, last_head_check }`.
- HEAD re-probe is debounced to 500 ms (`HEAD_PROBE_DEBOUNCE`).
- On HEAD-SHA change (`maybe_refresh_head`), the full `paths` cache is cleared and `index` is rebuilt.

The original review concern — that a delete-then-recreate at the same path could serve stale data — is bounded by two observations:

1. Git intelligence draws from commit history, not filesystem state. A cached insight is "stale" only if a new commit has changed the file's history for that path.
2. New commits move HEAD; HEAD moves trigger `paths.clear()`. The residual window is the 500 ms probe debounce plus any delay between commit and the next `resolve_path` call.

The residual window matters only for callers that straddle a commit: chiefly long-running MCP sessions doing back-to-back card reads around a `synrepo reconcile`. Even then, the failure mode is "one stale read, then self-heals on the next lookup after 500 ms." This is acceptable for current SLAs but worth documenting and test-locking so future cache refactors cannot silently regress it.

## Goals / Non-Goals

**Goals:**

- Lock the HEAD-change-invalidation guarantee with an integration test.
- Close the "multiple card-compile passes within one MCP session straddle a commit" window by forcing a HEAD probe at the start of each card-compile pass.
- Document the cache's invalidation contract so it cannot rot.

**Non-Goals:**

- No invalidation of per-entry on filesystem events. Card compilation runs on a snapshotted graph read; file-level events are the writer path's problem.
- No per-path fine-grained invalidation API. Clearing the whole cache on HEAD change is simpler and the rebuild cost is amortised across many lookups.
- No cross-layer import from `src/structure/identity.rs::IdentityResolution::Breakage`. Cards must not import from structure — `surface/` sits on top of `structure/`.
- No cache persistence across process restarts. The cache is per-process, per-`GraphCardCompiler`.
- No change to `PATH_CACHE_CAPACITY` (32,768) or `HEAD_PROBE_DEBOUNCE` (500 ms). Tuning is a separate concern.

## Decisions

### D1: Force-probe hook bound to `GraphCardCompiler`'s compile-pass boundary

Instead of adding a "clear on breakage" path that would couple surface to structure, add `GitCache::on_compile_cycle_end()` that resets `last_head_check` to a point in the past, guaranteeing the next `resolve_path` call takes the slow path and re-probes HEAD. This is the same mechanism already used by the test-only `force_head_probe` (lines 148–156), promoted from `#[cfg(test)]` to `pub(super)` and named for its production purpose.

Call site: wherever `GraphCardCompiler` ends a compile pass. For a single MCP request serving one card, the call happens after the card is built. For batch export flows (`synrepo export`), the call happens between files.

**Rationale**. This adds at most one additional `gix::open` + HEAD read per compile-pass boundary, which the `maybe_refresh_head` path already handles cheaply (git is reading the `.git/HEAD` file). Far cheaper than invalidating all cached paths on every `resolve_path` call, and it gets the cache back in sync with HEAD without cross-layer imports.

**Alternatives considered:**

- *Clear the whole `paths` map on each compile-pass boundary*: rejected — defeats the cache's purpose for the common case (single commit, many card reads). Only the residual "straddle a commit" case benefits.
- *Hook into `IdentityResolution::Breakage` from `pipeline::structural`*: rejected — breaks the layer architecture (cards sit above structure), and `Breakage` events are per-compile-pass, not per-card-read. Wrong granularity too.
- *Shorten `HEAD_PROBE_DEBOUNCE` from 500 ms to e.g. 50 ms*: rejected — 500 ms was chosen to absorb bulk-export traffic; lowering it trades throughput for narrower but still non-zero staleness window.

### D2: Integration test pins the delete-and-recreate scenario

Add `src/surface/card/compiler/git_cache/tests.rs::delete_and_recreate_invalidates_cache`:

1. Set up a `tempfile::TempDir` with a `git init`'d repo.
2. Commit `file.txt` with content "A". Run `GitCache::resolve_path(root, config, "file.txt")` and capture the resulting insights (non-empty commit list).
3. Delete `file.txt`, commit the deletion.
4. Re-create `file.txt` with content "B", commit.
5. Call `cache.on_compile_cycle_end()` to force the next probe.
6. Call `resolve_path` again. Assert the returned insights reflect the new commit history (two commit SHAs, not the original one).

**Rationale**. This is the most direct test that can be written; it exercises the full initialize → HEAD-change-refresh → clear → rebuild path against a real `GitHistoryIndex`. It uses `tempfile` (already a dev dep) and the project's existing git-test harness patterns.

**Alternatives considered:**

- *Unit-test `BoundedPathCache::clear()` in isolation*: already covered by module-internal tests; does not exercise the HEAD-change path.
- *Mock `GitHistoryIndex`*: the index is concrete, not trait-bound; mocking would require a boundary refactor that is larger than this change deserves.

### D3: Keep the test-only `force_head_probe` as-is

`force_head_probe` at lines 148–156 is `#[cfg(test)]` and exists for deterministic test scheduling without a clock abstraction. The new `on_compile_cycle_end` is the production variant of the same mechanism. Keep both — test code should not reach into production hooks that might later accrue side effects.

### D4: Module docstring rewrite

Rewrite `src/surface/card/compiler/git_cache/mod.rs:1-6` to four numbered invariants:

```
//! Per-compiler cache of file-scoped git-intelligence projections.
//!
//! Invariants:
//! 1. One `GitHistoryIndex` per HEAD SHA; rebuilt from scratch on SHA change.
//! 2. HEAD is re-probed at most every 500 ms (debounce). The probe may be
//!    forced at compile-cycle boundaries via `on_compile_cycle_end`.
//! 3. A SHA change clears the per-path memo before the next lookup.
//! 4. A `GitIntelligenceReadiness::Degraded { RepositoryUnavailable }`
//!    result during initialize latches the cache to `Inner::Unavailable`;
//!    subsequent lookups short-circuit to `None`.
```

**Rationale**. The current three-bullet docstring captures design choices; invariants are more useful for future maintainers. Keep it under ten lines.

## Risks / Trade-offs

- **`on_compile_cycle_end` is called in the wrong place or not at all**: mitigated by the integration test — if the hook is unwired, the test will see stale data. Also add a `debug_assert!` somewhere in `GraphCardCompiler` that verifies the hook fires per compile-pass (decide in implementation).

- **`on_compile_cycle_end` is called too aggressively**, causing excess HEAD probes: acceptable. Each probe reads `.git/HEAD` and compares against cached `head_sha`; no rebuild unless SHA moved. Cost is a single syscall plus a string compare.

- **Test relies on real `git` binary and `gix` crate**: already the case for the existing integration tests in `src/pipeline/git/`. No new test infrastructure needed.

- **Test is flaky on Windows due to file-deletion timing**: possible. Gate the test with `#[cfg(unix)]` if flakes appear, matching the pattern used by `tests/mutation_soak.rs`.

## Migration Plan

1. Add the `on_compile_cycle_end` method first with no call sites. Tests pass unchanged.
2. Wire the call from `GraphCardCompiler`. Existing card tests exercise the hook path.
3. Add the integration test last, after the hook is wired.
4. Rewrite the docstring.

No runtime migration. Existing caches in live MCP servers benefit on next restart; until then they behave the same as today.

## Open Questions

- **O1**: Where exactly does `GraphCardCompiler` sequence card-compile passes? The answer determines where `on_compile_cycle_end` is called. Resolve during implementation by reading `src/surface/card/compiler/mod.rs` and locating the top-level compile loop.
- **O2**: Is `debug_assert!`-enforcing the hook worth the noise? Probably not; the integration test covers correctness. Defer unless a regression appears.
