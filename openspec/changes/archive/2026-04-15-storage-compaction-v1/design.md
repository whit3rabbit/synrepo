## Context

synrepo has three categories of mutable storage that accumulate over time:

1. **Overlay store** (`.synrepo/overlay/`): commentary entries and cross-link candidates. Orphan pruning exists (removes entries whose endpoint nodes no longer exist), but there is no age-based retention. Stale commentary for live nodes persists indefinitely. Rejected and promoted cross-link audit rows accumulate without bound.
2. **State files** (`.synrepo/state/`): `repair-log.jsonl` is append-only. `reconcile-state.json` is overwritten each pass but has no rotation history.
3. **Lexical index** (`.synrepo/index/`): rebuildable cache. Already handled by the compatibility evaluator but not surfaced as an explicit operator action.

The existing `maintenance.rs` module plans and executes compatibility-driven actions (rebuild, clear-and-recreate) triggered by version skew. It does not handle retention policies or age-based compaction.

The graph store (`.synrepo/graph/nodes.db`) is canonical and must never be touched by compaction. This invariant is load-bearing.

## Goals / Non-Goals

**Goals:**
- Operator command (`synrepo compact`) that enforces retention policies on overlay, state, and index stores.
- Three policy presets: `default` (conservative), `aggressive` (shorter windows), `audit-heavy` (longer audit retention, minimal summarization).
- WAL checkpoint for both SQLite databases as part of the compact pass.
- Bounded repair-log retention with optional summarization of old entries.
- `synrepo status` reports compactable row counts and last compaction timestamp.
- Test proving compaction never touches canonical graph rows.

**Non-Goals:**
- Canonical graph compaction or pruning. The graph is never compacted semantically.
- Automatic scheduled compaction. The operator runs it explicitly or it could be wired into watch later.
- Configurable per-policy retention parameters. The three presets cover the use case; custom tuning is future work.
- Compaction of `.synrepo/cache/` or other transient directories not yet in the storage layout.

## Decisions

### Decision 1: Compact as a subcommand of the existing maintenance module

The compact command extends `maintenance.rs` with a new `CompactPlan` and `execute_compact` function rather than introducing a new top-level module. The existing `MaintenanceAction` enum and `plan_maintenance` function handle compatibility-driven maintenance. Compact adds a parallel path for retention-driven maintenance.

**Why**: Avoids a new module when the concerns overlap heavily (both plan and execute store-level actions). The existing pattern of plan-then-execute with dry-run default is already established.

**Alternative considered**: A standalone `src/pipeline/compact.rs` module. Rejected because it would duplicate the plan/execute/status pattern and create confusion about which module owns store maintenance.

### Decision 2: Overlay trait extension for age-based queries

Add two methods to `OverlayStore`:

```rust
fn compactable_commentary_stats(&self, policy: &CompactPolicy) -> crate::Result<CompactStats>;
fn compact_commentary(&mut self, policy: &CompactPolicy) -> crate::Result<CompactSummary>;

fn compactable_cross_link_stats(&self, policy: &CompactPolicy) -> crate::Result<CompactStats>;
fn compact_cross_links(&mut self, policy: &CompactPolicy) -> crate::Result<CompactSummary>;
```

These methods encapsulate the policy logic (age thresholds, audit summarization) in the overlay implementation, keeping the maintenance module as a coordinator.

**Why**: The overlay implementation knows its own schema. Exposing age-based queries through the trait keeps the policy logic testable at the store level without the maintenance module needing SQL knowledge.

### Decision 3: Policy as an enum, not a config section

`CompactPolicy` is a three-variant enum (`Default`, `Aggressive`, `AuditHeavy`) with hard-coded retention thresholds, not a set of configurable fields in `config.toml`.

**Why**: Three presets cover the design space. Exposing every threshold as config creates a support burden without a clear use case. If custom tuning is needed later, a `Custom` variant with explicit thresholds can be added without breaking the preset variants.

### Decision 4: Repair-log rotation via file truncation with summary header

Repair-log compaction reads the JSONL file, summarizes entries older than the retention window into a single header line (counts by surface and action), and rewrites the file with the summary followed by retained entries.

**Why**: The JSONL format has no index, so partial truncation requires a full rewrite. The summary header preserves aggregate history without retaining every row. This is consistent with how the log is already consumed (bounded by limit in diagnostics).

### Decision 5: WAL checkpoint runs after compaction, not before

The compact pass executes all retention actions first, then runs `PRAGMA wal_checkpoint(TRUNCATE)` on both graph and overlay databases. The graph checkpoint is included because it reclaims disk space from normal write traffic, even though compaction does not modify graph rows.

**Why**: Running the checkpoint after compaction ensures the WAL file reflects only the post-compaction state, minimizing WAL size. Including the graph checkpoint is free (read-only on graph data) and benefits the operator.

## Risks / Trade-offs

- **Risk**: Overlay compaction deletes commentary that an agent might still reference. **Mitigation**: The `default` policy retains stale commentary for a 30-day review window. Dry-run is the default mode. The policy enum makes the tradeoff explicit.
- **Risk**: Repair-log rewrite could lose entries if the process crashes mid-write. **Mitigation**: Write to a temporary file and rename atomically (same pattern as reconcile-state).
- **Risk**: WAL checkpoint on the graph database could block briefly under heavy write load. **Mitigation**: Compaction is an operator-initiated command, not automatic. The busy_timeout (5s) handles transient contention.
- **Trade-off**: Audit summarization for old cross-link entries loses per-row detail. The `audit-heavy` policy minimizes this by extending the retention window to 180 days before summarization.
