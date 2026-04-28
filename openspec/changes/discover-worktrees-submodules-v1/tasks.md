# Worktree and submodule discovery — tasks

## 1. Research and scoping

- [x] Survey the `gix::Repository::worktrees()` and `gix::Repository::submodules()` surface. Document the available fields (head commit, active branch, submodule URL, submodule path, nested-submodule recursion policy) in `design.md`.
- [x] Decide submodule policy: recurse (walk submodule trees as additional roots) vs. opaque (record the mount point and skip). Default recommendation: **opaque**, with `include_submodules: true` opt-in. Document in `design.md` Decision 1.
- [x] Decide `FileNodeId` discriminant shape. Candidates: (a) salt content hash with absolute root path, (b) salt with `.git/worktrees/<name>/HEAD` SHA, (c) introduce a root-id type field. See `design.md` Decision 2.

## 2. Identity model

- [x] Extend `derive_file_id` in `src/core/ids.rs` to accept a root discriminant. Preserve invariant 3: ID remains stable across renames within a root.
- [x] Update callers of `derive_file_id` to pass the discriminant from the root that owns the file. `src/pipeline/structural/` is the main call site.
- [x] Store the root discriminant alongside each `FileNode` (schema change in `src/store/sqlite/schema.rs`).
- [x] Bump the compatibility version in `src/store/compatibility/version.rs` and document the migration in `src/store/compatibility/migrations/`.

## 3. Discovery

- [x] In `src/substrate/discover.rs`, introduce `DiscoveryRoot { absolute_path, discriminant, kind }` with `kind ∈ {Primary, Worktree, Submodule}`.
- [x] Enumerate worktrees via `gix::Repository::worktrees()`. Treat linked worktrees as additional `DiscoveryRoot`s when `config.include_worktrees` is true.
- [x] Enumerate submodules via `gix::Repository::submodules()`. Follow policy from Decision 1.
- [x] Update the per-root walk to respect each root's own `.gitignore`, `.synignore`, and redaction globs.

## 4. Pipeline changes

- [x] Multi-root history mining in `src/pipeline/git/mod.rs`. Each root gets its own sampled commit history; cross-root edges do not exist.
- [x] Per-root reconcile in `src/pipeline/watch/reconcile.rs`. A file event scopes reconcile to the owning root only.
- [x] Watch service per-root event scoping in `src/pipeline/watch/service.rs`.

## 5. Config

- [x] Add `include_worktrees: bool` (default `true`) to `Config` in `src/config/mod.rs`. Serde alias for backwards compatibility.
- [x] Add `include_submodules: bool` (default `false` pending Decision 1).
- [x] Document both fields in `docs/CONFIG.md`.

## 6. Tests

- [x] Fixture: a repo with `git worktree add ../wt` in place, both trees exercised.
- [x] Fixture: a repo with a submodule, both `include_submodules: true` and `false` variants.
- [x] Identity test: same file content in two worktrees produces two distinct `FileNodeId`s (no collapse).
- [x] Identity test: same file renamed within ONE worktree keeps its `FileNodeId` (invariant 3 preserved).
- [x] Watch test: a write in worktree A does not trigger a reconcile for worktree B.

## 7. Documentation

- [x] Update `docs/ARCHITECTURE.md` discovery section.
- [x] Update `AGENTS.md` "Git and watch" gotcha section with the new multi-root semantics.
- [x] Update `docs/ADDING-LANGUAGE.md` if per-language fixture behavior changes (likely no-op).
