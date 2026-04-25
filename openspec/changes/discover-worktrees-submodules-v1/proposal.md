## Why

Filesystem discovery treats the working tree as a single git checkout. Linked git worktrees and git submodules are not walked. From `src/substrate/discover.rs:32-37`:

```
/// Phase 0 implementation: honors `.gitignore` via the `ignore` crate,
/// applies size cap, applies redaction globs, sniffs encoding. Does not
/// yet integrate with git worktrees or submodules, which remains phase 1.
```

A repo created via `git worktree add ../feature` or with `git submodule add ./sub` produces a partial index: the linked worktree's content is absent, and submodule files resolve against the submodule's own history rather than the parent's.

The gap surfaced in an audit of deferred work and is queued here rather than bundled into unrelated repair work.

## What Changes

- Discovery walks `.git/worktrees/*/HEAD` entries and follows each linked worktree's filesystem as an additional root. Per-worktree ownership is recorded so reconcile, watch, and identity layers can scope events correctly.
- Submodule recognition: parse `.gitmodules` and either (a) walk each submodule's tree as an additional root, producing separate `FileNode`s keyed per-root, or (b) treat submodules as opaque mount points and skip their contents. Decision deferred to `design.md`.
- Identity model: every `FileNode` gains a root-discriminant so two checkouts of the same upstream content do not collapse into a single `FileNodeId` (which would violate invariant 3 — `FileNodeId` is stable across renames but NOT across physically distinct tree checkouts).
- Git history mining in `src/pipeline/git/mod.rs` gains multi-root awareness so per-worktree HEADs are sampled independently.
- Watch service gains per-root event scoping so a write in one worktree does not trigger a re-compile in a sibling worktree.

## Capabilities

### New Capabilities

None. Refines existing discovery, identity, and watch capabilities.

### Modified Capabilities

- `substrate-discovery`: walk includes linked worktrees and (configurably) submodules.
- `graph-identity`: `FileNodeId` derivation salts with a per-root discriminant to prevent collapse across physically distinct trees with identical content.
- `git-intelligence`: history mining respects per-root HEADs.
- `watch-and-ops`: file-event scoping per-root.

## Impact

- `src/substrate/discover.rs` — multi-root walker: enumerate worktrees via `gix::Repository::worktrees()`, enumerate submodules via `gix::Repository::submodules()`, apply configured policy.
- `src/core/ids.rs::derive_file_id` — salt the content hash with a per-root discriminant (a stable hash of the root's absolute path, or the `.git/worktrees/*/HEAD` SHA for linked worktrees).
- `src/pipeline/git/mod.rs` — multi-root history mining.
- `src/pipeline/watch/service.rs` — per-root event scoping; a file event fires reconcile only for the root that owns the affected path.
- `src/store/compatibility/version.rs` — schema-bump required because `FileNode`'s identity domain changes. Existing graphs stay usable but need a migration step.
- `src/config/mod.rs` — new `include_submodules: bool` config (default `false` until the design decision finalizes) and `include_worktrees: bool` (default `true` since the gap is here).
- Tests: per-worktree and per-submodule fixtures under `src/substrate/` and `src/pipeline/structural/tests/`. Existing fixtures assume a single root; they stay unchanged.
