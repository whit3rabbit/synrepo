## 1. Card types

- [x] 1.1 In `src/surface/card/types.rs`, add `LastChangeGranularity` enum (`File`, `Symbol`, `Unknown`) with `#[serde(rename_all = "snake_case")]`
- [x] 1.2 In `src/surface/card/types.rs`, add `SymbolLastChange` struct (`revision`, `author_name`, `committed_at_unix`, `granularity`, `summary: Option<String>`) with `summary` using `#[serde(skip_serializing_if = "Option::is_none")]`
- [x] 1.3 Change `SymbolCard.last_change` from `Option<String>` to `Option<SymbolLastChange>`
- [x] 1.4 Add `impl SymbolLastChange { fn from_file_intelligence(&FileGitIntelligence, Budget) -> Option<Self> }` in `src/surface/card/git.rs` (or a new module) projecting the most recent commit, gated on readiness and non-empty commits
- [x] 1.5 Run `cargo check` to confirm no consumer of the old string shape remains

## 2. Card compiler plumbing

- [x] 2.1 Add `GitCacheState` enum (`Uninitialized`, `Unavailable`, `Ready { context, by_path }`) in a new file `src/surface/card/compiler/git_cache.rs` (stay under 400 lines) with a `resolve_path(&mut self, repo_root, path, depth, max_results) -> Option<Arc<FileGitIntelligence>>` method
- [x] 2.2 Add `git_cache: parking_lot::Mutex<GitCacheState>` field to `GraphCardCompiler`
- [x] 2.3 Initialize the cache as `Uninitialized` in `GraphCardCompiler::new`
- [x] 2.4 Expose an internal helper on the compiler (`fn git_intel_for(&self, path: &str) -> Option<Arc<FileGitIntelligence>>`) that locks the mutex, opens the context on first use, walks history on first lookup per path, and caches `Unavailable` on open failure with a single `tracing::warn!`
- [x] 2.5 Rewrite the stale doc comment at `src/surface/card/compiler/mod.rs:8-9` so it reflects the post-change behavior (remove the `git-intelligence-v1` reference, describe degraded and null surfacing)

## 3. FileCard populator

- [x] 3.1 In `src/surface/card/compiler/file.rs`, thread the compiler handle through `file_card` so it can call `git_intel_for` (extend the `file_card` signature or add a thin wrapper)
- [x] 3.2 For `Budget::Tiny`, leave `git_intelligence: None`
- [x] 3.3 For `Budget::Normal` and `Budget::Deep`, set `git_intelligence` to the cached entry's inner value (cloning from `Arc<FileGitIntelligence>`); on `GitCacheState::Unavailable`, leave `None`
- [x] 3.4 Ensure `estimate_tokens_file` accounts for the new payload so `approx_tokens` stays sane (walk `commits`, `co_change_partners`, ownership string, status enum tag)

## 4. SymbolCard populator

- [x] 4.1 In `src/surface/card/compiler/symbol.rs`, extend `SymbolCardContext` (or parallel to it) with a handle to call `git_intel_for(file.path)`
- [x] 4.2 For `Budget::Tiny`, leave `last_change: None`
- [x] 4.3 For `Budget::Normal` and `Budget::Deep`, call `SymbolLastChange::from_file_intelligence(&entry, budget)` and assign; `None` when the helper returns `None` (degraded or unavailable)
- [x] 4.4 `summary` must be populated only for `Deep`; `revision` must be short-form at `Normal` (first 7 hex chars) and full at `Deep`
- [x] 4.5 Ensure `estimate_tokens_symbol` accounts for the new payload

## 5. Tests

- [x] 5.1 Update existing snapshot tests in `src/surface/card/compiler/tests.rs` to cover the new `git_intelligence` and `last_change` shapes; set up a temp-git-repo fixture (already a pattern in the git intelligence tests — reuse the helper from `src/pipeline/git_intelligence/tests/`)
- [x] 5.2 Add a snapshot test: `FileCard.git_intelligence` populated at `Normal` with a known two-commit fixture
- [x] 5.3 Add a snapshot test: `SymbolCard.last_change` at `Normal` (no summary) and `Deep` (with summary) from the same fixture
- [x] 5.4 Add a unit test: `GitCacheState` caches `Unavailable` after open failure and does not re-attempt (inject a non-git directory)
- [x] 5.5 Add a unit test: degraded readiness produces a `FileCard.git_intelligence` with non-`ready` status and a `null` or `Unknown`-granularity `SymbolCard.last_change`
- [x] 5.6 Add a unit test: a repo without git produces `FileCard.git_intelligence: None` and `SymbolCard.last_change: None`
- [x] 5.7 Regenerate all `src/surface/card/compiler/snapshots/*.snap` via `cargo insta test --review` and confirm the diff matches the design

## 6. Validation

- [x] 6.1 `cargo clippy --workspace --all-targets -- -D warnings` clean
- [x] 6.2 `cargo test` green
- [x] 6.3 `make check` green
- [x] 6.4 `cargo run -- export --deep --out /tmp/synrepo-context` on a real synrepo checkout produces a `FileCard` payload with `git_intelligence` populated and at least one `SymbolCard.last_change` with `granularity: "file"`
- [x] 6.5 `openspec validate git-data-surfacing-v1 --strict` passes

## 7. Documentation touch-ups (in-scope)

- [x] 7.1 Remove the "`FileCard.git_intelligence` wiring" bullet from ROADMAP.md §11.1 "Track D / I — data computed but not surfaced"
- [x] 7.2 Remove the "`SymbolCard.last_change` field does not exist" bullet from ROADMAP.md §11.1 (the field will exist in its new shape)
- [x] 7.3 Update CLAUDE.md / AGENTS.md if it still claims `FileCard.git_intelligence` is `None` unconditionally
