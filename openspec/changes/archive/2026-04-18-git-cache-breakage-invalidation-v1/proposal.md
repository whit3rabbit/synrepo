## Why

The original review flagged a concern that `BoundedPathCache` in `src/surface/card/compiler/git_cache/mod.rs` is FIFO-by-capacity with no invalidation hook for file deletions, which could cause stale git metadata to surface when a file is recreated at the same path.

Phase 1 inspection of the module shows the concern is mostly addressed already: `maybe_refresh_head` (lines 194–249) debounces at 500 ms, reads the repo HEAD, and calls `paths.clear()` on any SHA change (line 237), which fully invalidates every cached path entry. Because git intelligence reflects committed history, a path's cached insights remain correct under the current HEAD regardless of live filesystem state.

The residual risk is narrow and specific: between `GraphCardCompiler` sessions (or during a single session if the operator commits rapidly and the 500 ms debounce masks a probe), a path could in principle serve its previous-generation insights for one cache read. This has not caused any observed incident, and the cache design was explicitly chosen for throughput on bulk-export paths.

Rather than add a new invalidation hook that duplicates the HEAD-SHA path, this change pins the HEAD-change-invalidation contract with an integration test and documents the guarantee so future refactors cannot silently drop it. It also adds a hardening tweak: force a probe on the first `resolve_path` call after a `GraphCardCompiler` is constructed, so the initial `Ready` state always reflects the repo's current HEAD rather than whatever it was at `GitCache::new()`.

## What Changes

- Add an integration test in `src/surface/card/compiler/git_cache/tests.rs` that exercises the delete-and-recreate-at-same-path scenario: commit a file, read its insights, replace the file's content and commit again, observe that the second read returns the new history rather than the cached prior generation.
- Add a `GitCache::on_compile_cycle_end()` method (or equivalent low-cost no-op hook) that `GraphCardCompiler` invokes between card-compile passes. The default behaviour is to force the next `resolve_path` call to probe HEAD (bypassing the 500 ms debounce for that one call). This closes the narrow "rapid-commit inside a single session" window without paying per-lookup cost.
- Update the module docstring at `src/surface/card/compiler/git_cache/mod.rs:1-6` to describe the HEAD-change invalidation contract, the 500 ms debounce window, and the compile-cycle force-probe hook, so the invariant is discoverable.
- Add tracing at `debug` level when the force-probe is triggered, for observability.

This change deliberately does NOT wire the cache to `IdentityResolution::Breakage` events in `src/structure/identity.rs`. Breakage is a pipeline-layer signal about structural compile state; the git cache lives on the cards/surface layer and is already correctly driven by git HEAD. Crossing that layer boundary would violate the architecture's no-upward-imports rule and duplicate invalidation logic that already works.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cards`: The file-scoped git intelligence provided by `GitCache` gains a documented, test-locked HEAD-invalidation guarantee. No change to card output shape.

## Impact

- **Code**:
  - `src/surface/card/compiler/git_cache/mod.rs` — new `on_compile_cycle_end()` method on `GitCache`, docstring rewrite, tracing line.
  - `src/surface/card/compiler/git_cache/tests.rs` — new delete-and-recreate integration test.
  - `src/surface/card/compiler/mod.rs` (or wherever `GraphCardCompiler` sequences card-compile passes) — call `on_compile_cycle_end()` between passes.
- **APIs**: Internal. `on_compile_cycle_end` is `pub(super)` (same visibility as the rest of `GitCache`'s surface).
- **Dependencies**: None.
- **Systems**: None.
- **Docs**: Updated module docstring; no changes to `AGENTS.md` / `CLAUDE.md` gotchas since this preserves current semantics and only tightens them.
