## 1. Locate compile-pass boundaries

- [x] 1.1 Read `src/surface/card/compiler/mod.rs` and identify the function(s) that sequence card-compile passes (answer to design open question O1).
- [x] 1.2 Record the chosen call-site path and function name in an implementation note; this drives task 3.1.

## 2. Add `on_compile_cycle_end` to `GitCache`

- [x] 2.1 Promote the mechanism in `force_head_probe` (`src/surface/card/compiler/git_cache/mod.rs:148-156`) to a production method.
- [x] 2.2 Add `pub(super) fn on_compile_cycle_end(&self)` that acquires the write lock, matches `Inner::Ready`, and rewinds `last_head_check` to before `HEAD_PROBE_DEBOUNCE`.
- [x] 2.3 Add a `tracing::debug!` line inside the method so force-probes are observable.
- [x] 2.4 Keep `force_head_probe` in place for tests; do not merge or remove it.
- [x] 2.5 Run `cargo check -p synrepo` and confirm no warnings from the new method.

## 3. Wire the hook into `GraphCardCompiler`

- [x] 3.1 At the boundary identified in task 1.2, call `git_cache.on_compile_cycle_end()` after each compile pass completes.
- [x] 3.2 If the compiler holds the cache via `Arc`/`&`, ensure the call is cheap (no clone of large state).
- [x] 3.3 Run `cargo test --lib surface::card::compiler::` and confirm existing card tests stay green.

## 4. Add the delete-and-recreate integration test

- [x] 4.1 In `src/surface/card/compiler/git_cache/tests.rs`, add `delete_and_recreate_invalidates_cache` per design D2.
- [x] 4.2 Use `tempfile::TempDir` to set up a fresh git repo; drive `git2`/`gix` to create two commits (or use `std::process::Command` to shell out to `git` if that matches existing test patterns — check the git tests in `src/pipeline/git/` for the convention).
- [x] 4.3 Call `GitCache::new()`, resolve the path once, perform the delete-commit-recreate-commit sequence, call `on_compile_cycle_end()`, resolve the path again, assert the new history is served.
- [x] 4.4 Run `cargo test --lib surface::card::compiler::git_cache::tests::delete_and_recreate_invalidates_cache`.
- [x] 4.5 If the test is flaky on CI, gate with `#[cfg(unix)]` and mirror the pattern from `tests/mutation_soak.rs`.

## 5. Rewrite the module docstring

- [x] 5.1 Replace lines 1–6 of `src/surface/card/compiler/git_cache/mod.rs` with the four-invariant block from design D4.
- [x] 5.2 Run `cargo doc --no-deps --lib` and confirm the new docstring renders without warnings.

## 6. Verification

- [x] 6.1 Run `make check` and confirm fmt, clippy, and the full test suite pass.
- [x] 6.2 Run the new integration test under `cargo test --lib surface::card::compiler::git_cache::tests::delete_and_recreate_invalidates_cache --test-threads=1` to rule out parallel-git-contention flakes.
- [x] 6.3 Smoke-test: `cargo run -- init` against a repo, then commit a file change, then `cargo run -- export` and inspect a FileCard's `git_intelligence` to confirm it reflects the new commit.

## 7. Archive

- [x] 7.1 Run `openspec validate git-cache-breakage-invalidation-v1 --strict`.
- [x] 7.2 Invoke `opsx:archive` with change id `git-cache-breakage-invalidation-v1`.
