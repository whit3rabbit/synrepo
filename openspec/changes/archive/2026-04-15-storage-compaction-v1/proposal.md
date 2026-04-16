## Why

synrepo's overlay store, repair-log, and lexical index accumulate stale rows and unbounded history over time. There is no operator command to enforce the retention policies that Milestone D describes. Without it, overlay cost grows without bound, repair-log becomes noisy, and `synrepo status` cannot report compactable volume. The existing `prune_orphans` hook in the overlay trait removes dead-node references, but it runs only during reconcile and has no policy layer for age-based retention or audit summarization.

## What Changes

- New CLI command: `synrepo compact` with `--dry-run` (default) and `--apply` modes, plus `--policy default|aggressive|audit-heavy`.
- Policy-driven compaction for overlay commentary (keep latest per node, retain stale copies for a review window, drop very old), cross-link candidates (keep active, retain promoted/rejected for audit period, summarize older rows into counts), and findings (retain actionable, expire superseded informational).
- Repair-log rotation: full fidelity for a short window, summarization or truncation for older entries.
- Lexical index rebuild as a compact sub-action (already supported by the compatibility evaluator, surfaced here as an explicit operator choice).
- SQLite WAL checkpoint (PRAGMA wal_checkpoint(TRUNCATE)) as part of the compact pass for both graph and overlay databases.
- `synrepo status` gains compactable-row counts and last-compaction timestamp.
- Scope boundary enforced: compaction never touches canonical graph rows (nodes, edges, provenance). It only targets overlay, state files, and the index.

## Capabilities

### New Capabilities

(none — the compact command is an operator surface within existing capabilities)

### Modified Capabilities

- `storage-and-compatibility`: adds compact-specific maintenance actions (wal-checkpoint, overlay-retention, index-rebuild) to the maintenance plan beyond the existing upgrade-driven compatibility actions.
- `watch-and-ops`: adds the `synrepo compact` command contract, policy definitions, and status counters to the operational lifecycle surface.
- `repair-loop`: adds repair-log rotation as a repair-surface concern (stale-log detection, log compaction as a sync action).

## Impact

- New code in `src/pipeline/maintenance.rs` (policy evaluation, retention actions) and `src/bin/cli.rs` / `src/bin/cli_support/` (compact subcommand).
- Existing `OverlayStore` trait may need new methods for age-based row queries and bulk deletion.
- `synrepo status` output gains new fields (compactable counts, last compact timestamp).
- No breaking changes to existing commands or store formats.
- Dependencies: none new. Uses existing SQLite, overlay, and compatibility infrastructure.
