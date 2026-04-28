# Store format v2: root-discriminated files

Graph format v2 changes file identity and path uniqueness from a single-root
domain to a root-aware domain:

- `FileNode` stores `root_id`, the stable discriminator for the owning
  discovery root. The primary checkout uses `primary`; linked worktrees and
  submodules use a stable hash of the owning root's canonical absolute path.
- `derive_file_id` salts the first-seen content hash with `root_id`.
- SQLite `files` rows are unique by `(root_id, path)`, not by `path` alone.

Existing graph stores written by v1 must be migrated or rebuilt before use.
The compatibility layer reports older canonical graph stores as
`migrate-required` after the version bump. A migration can assign all existing
rows to the primary root, rewrite file IDs with the new derivation, and then
re-run reconcile to repopulate dependent symbol and edge identifiers.
