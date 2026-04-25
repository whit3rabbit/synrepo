# Design: Git worktree and submodule discovery

## Decision 1 — Submodule policy

Default: **opaque** (record the mount point, do not walk). Rationale:

- Submodules are commonly used for vendored dependencies, docs generators, or test fixtures that are not considered part of the parent project's code surface. Walking them would flood the graph with unrelated symbols and inflate token budgets for agent cards.
- Recursion into submodules also introduces cross-root edges (e.g., a `Calls` edge from parent into a submodule function), which contradicts the per-root identity isolation needed to keep `FileNodeId` stable.
- When a user wants the submodule's content indexed, the submodule itself can be a separate synrepo target.

Expose `include_submodules: bool` (default `false`) as the opt-in. When `true`, each submodule root is enumerated identically to a linked worktree — as an additional `DiscoveryRoot` with its own discriminant.

## Decision 2 — `FileNodeId` discriminant shape

Salt the content hash with a **stable hash of the root's canonical absolute path**, resolved via `std::fs::canonicalize`. Rationale:

- Preserves invariant 3 (`FileNodeId` stable across renames within a root) because renames keep the same root path.
- Two physical checkouts of the same upstream (e.g., `/home/user/project` and `/home/user/project-wt-feature`) produce distinct IDs even when file content is byte-identical, preventing collision.
- Linked-worktree path is stable across a session (worktree paths are part of git's metadata and rarely change).
- Alternative (`.git/worktrees/<name>/HEAD` SHA): rejected because the HEAD SHA changes on every commit, invalidating IDs every time the worktree moves forward — catastrophic for drift scoring.

Schema change: store the discriminant alongside each `FileNode` as a `root_id` column in the SQLite schema. Bump compatibility version; existing graphs fall back to "primary root" via migration.

## Decision 3 — Worktree inclusion default

Default `include_worktrees: true`. Rationale:

- Linked worktrees are a native git feature used by the project itself (see `.claude/worktrees/` in the repo). Agents reasoning about the project already operate across worktrees.
- The cost of walking them is low (they're on the same filesystem, usually one extra directory tree).
- Per-root identity isolation (Decision 2) means enabling this does not break existing graphs; it just makes previously invisible files discoverable.

## Decision 4 — Per-root reconcile scoping

The watch service tracks `(root_discriminant, changed_paths)` tuples. When an event fires:

- The event's absolute path is matched against each root's absolute path prefix. Longest-prefix wins (submodule inside worktree wins over worktree).
- Reconcile runs only for the owning root. Other roots' `FileNode`s and `SymbolNode`s remain untouched.

Cross-root edges are not emitted in this iteration. `EdgeKind::CoChangesWith`, `Imports`, `Calls` all stay within a single root. A follow-on change can relax this for `CoChangesWith` (which is git-observed) if there is demand.

## Decision 5 — Migration path

First `synrepo upgrade --apply` after this code lands:

1. Detects the schema bump in `.synrepo/graph/nodes.db`.
2. Re-runs discovery with primary-root-only semantics (equivalent to pre-change behavior) to assign existing `FileNode`s a root discriminant matching the current working tree.
3. Rewrites `FileNodeId`s in place to include the discriminant. This is a one-time cost amortized over `retain_retired_revisions` cycles.

Rollback-unsafe: once IDs are rewritten, an older binary cannot read the graph. The compatibility block already handles this case (`synrepo init` and graph commands error when the recorded format version is newer).

## Open questions

1. **Nested submodules** — if a submodule has its own submodules, do we recurse? Proposed: yes, recursively, bounded by a configurable depth limit (default 3).
2. **Branch switching inside linked worktrees** — when a linked worktree's HEAD moves, do we treat that as a rename or as unrelated content? Current stance: the discriminant is the root path, not HEAD, so branch switches are invisible to identity. This matches primary-root behavior.
3. **Worktree removal** — when a linked worktree is deleted with `git worktree remove`, does the next reconcile retire its files? Proposed: yes, using the standard "disappeared files" flow (stage 6 identity cascade), scoped per-root.
