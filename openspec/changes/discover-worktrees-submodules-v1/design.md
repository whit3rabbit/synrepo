# Design: Git worktree and submodule discovery

## `gix` API survey

Verified against `gix = 0.82.0` from this repository's lockfile.

`Repository::worktrees()` returns only linked worktrees, not the main worktree. Each item is a `gix::worktree::Proxy` sorted by private git-dir path. The proxy exposes:

- `base() -> io::Result<PathBuf>`: checkout root, derived from the linked worktree's `gitdir` file. The path might not exist anymore.
- `git_dir() -> &Path`: private linked-worktree git dir, usually under the main repository's `.git/worktrees/<id>`.
- `id() -> &BStr`: linked worktree id, derived from that private git-dir folder name.
- `is_locked()` and `lock_reason()`: prune/move/delete lock metadata.
- `into_repo()` and `into_repo_with_possibly_inaccessible_worktree()`: open the linked worktree as a `gix::Repository`.

The worktree proxy does not directly expose an active branch or head commit as fields. Open the proxy as a repository and query `head()` / `head_id()` style repository APIs when history mining needs the current head.

`Repository::submodules()` returns `Result<Option<impl Iterator<Item = gix::Submodule<'_>>>>`. It reads `.gitmodules` from the worktree when present, otherwise it can fall back through the index or current tree via `modules()`. Each `Submodule` exposes:

- `name()` and `validated_name()`: configured name and safe path component validation.
- `path()`: path relative to the superproject worktree.
- `url()`: configured clone/update URL, with configuration overrides applied.
- `branch()`: optional configured tracking branch. This is configuration, not necessarily the checked-out submodule HEAD.
- `fetch_recurse()`: configured `fetchRecurseSubmodules` value, falling back to `fetch.recurseSubmodules`.
- `is_active()`: active-submodule policy evaluation.
- `index_id()` and `head_id()`: gitlink commit recorded in the superproject index or HEAD tree.
- `git_dir()`, `work_dir()`, `git_dir_try_old_form()`, `state()`, and `open()`: paths/state/opening for initialized submodule repositories.

Nested submodule recursion is not automatic. To recurse when `include_submodules` is enabled, open a submodule repository with `Submodule::open()` and enumerate its own `submodules()` with an explicit depth limit. This change uses a default depth of 3 for the planned implementation.

## Decision 1 — Submodule policy

Default: **opaque** (record the mount point, do not walk). Rationale:

- Submodules are commonly used for vendored dependencies, docs generators, or test fixtures that are not considered part of the parent project's code surface. Walking them would flood the graph with unrelated symbols and inflate token budgets for agent cards.
- Recursion into submodules also introduces cross-root edges (e.g., a `Calls` edge from parent into a submodule function), which contradicts the per-root identity isolation needed to keep `FileNodeId` stable.
- When a user wants the submodule's content indexed, the submodule itself can be a separate synrepo target.

Expose `include_submodules: bool` (default `false`) as the opt-in. When `true`, each submodule root is enumerated identically to a linked worktree — as an additional `DiscoveryRoot` with its own discriminant.

## Decision 2 — `FileNodeId` discriminant shape

Salt the content hash with a stable root discriminator. The primary checkout uses the literal discriminator `primary`; linked worktrees and submodules use a stable hash of the root's canonical absolute path, resolved via `std::fs::canonicalize`. Rationale:

- Preserves invariant 3 (`FileNodeId` stable across renames within a root) because renames keep the same root path.
- Two physical checkouts of the same upstream (e.g., `/home/user/project` and `/home/user/project-wt-feature`) produce distinct IDs even when file content is byte-identical, preventing collision.
- The primary checkout remains deterministic across tempdir-backed tests and runtime migration. A single graph has only one primary root, so `primary` is sufficient to separate it from linked roots.
- Linked-worktree path is stable across a session (worktree paths are part of git's metadata and rarely change).
- Alternative (`.git/worktrees/<name>/HEAD` SHA): rejected because the HEAD SHA changes on every commit, invalidating IDs every time the worktree moves forward — catastrophic for drift scoring.

Schema change: store the discriminant alongside each `FileNode` as a `root_id` column in the SQLite schema. Bump compatibility version; existing graphs fall back to "primary root" via migration.

Path uniqueness also moves from `path` alone to `(root_id, path)`. This is required because the primary checkout and a linked worktree commonly contain the same repo-relative path, for example `src/lib.rs`. Public path-only lookups may remain as compatibility shims for single-root callers, but structural compile, discovery, and watch scoping must use root-aware lookup APIs.

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
