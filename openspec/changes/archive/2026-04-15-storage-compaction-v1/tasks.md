## 1. Types and Policy

- [x] 1.1 Define `CompactPolicy` enum (`Default`, `Aggressive`, `AuditHeavy`) with hard-coded retention thresholds in `src/pipeline/maintenance.rs` (or a new `src/pipeline/compact.rs` if maintenance.rs exceeds 400 lines after additions)
- [x] 1.2 Define `CompactPlan`, `CompactAction`, and `CompactSummary` structs for planning and reporting compact results
- [x] 1.3 Add `CompactStats` struct (compactable commentary count, compactable cross-link audit count, repair-log entries beyond window, last compaction timestamp)

## 2. Overlay Store Extension

- [x] 2.1 Add `compactable_commentary_stats(&self, policy: &CompactPolicy) -> Result<CompactStats>` method to `OverlayStore` trait in `src/overlay/mod.rs`
- [x] 2.2 Add `compact_commentary(&mut self, policy: &CompactPolicy) -> Result<CompactSummary>` method to `OverlayStore` trait
- [x] 2.3 Add `compactable_cross_link_stats(&self, policy: &CompactPolicy) -> Result<CompactStats>` method to `OverlayStore` trait
- [x] 2.4 Add `compact_cross_links(&mut self, policy: &CompactPolicy) -> Result<CompactSummary>` method to `OverlayStore` trait
- [x] 2.5 Implement the four new trait methods in `src/store/overlay/` (SQLite-backed age queries and bulk deletion)
- [x] 2.6 Write tests: compactable stats reflect stale vs active entries, compaction drops only stale entries within policy window, active candidates are never dropped

## 3. Repair-Log Rotation

- [x] 3.1 Implement `rotate_repair_log(synrepo_dir: &Path, policy: &CompactPolicy) -> Result<CompactSummary>` in the compact module
- [x] 3.2 Write tests: rotation summarizes old entries into a header line, preserves recent entries, is idempotent on already-compacted log, uses atomic file rewrite

## 4. WAL Checkpoint and Index Rebuild

- [x] 4.1 Add `wal_checkpoint` function that runs `PRAGMA wal_checkpoint(TRUNCATE)` on both graph and overlay SQLite databases
- [x] 4.2 Wire the existing index rebuild capability from the compatibility evaluator into the compact pass (reuse `MaintenanceAction::ClearAndRecreate` for the index store)
- [x] 4.3 Write test: WAL checkpoint completes without error on a populated database

## 5. Compact Plan and Execute

- [x] 5.1 Implement `plan_compact(synrepo_dir: &Path, config: &Config, policy: CompactPolicy) -> Result<CompactPlan>` that queries overlay stats, repair-log age, and index freshness
- [x] 5.2 Implement `execute_compact(synrepo_dir: &Path, plan: &CompactPlan, policy: CompactPolicy) -> Result<CompactSummary>` that runs all sub-actions in sequence and records the completion timestamp in `.synrepo/state/compact-state.json`
- [x] 5.3 Write integration test: full compact pass on a repo with stale commentary, old audit rows, and an aged repair-log; verify counts and file state after

## 6. CLI Surface

- [x] 6.1 Add `compact` subcommand to `src/bin/cli.rs` with `--apply` (default dry-run), `--policy <default|aggressive|audit-heavy>` flags
- [x] 6.2 Wire the compact subcommand to `plan_compact` (dry-run) or `execute_compact` (apply) and render the plan/summary to stdout
- [x] 6.3 Add compactable counts and last compaction timestamp to `synrepo status` output (both text and JSON modes)
- [x] 6.4 Write CLI smoke test: `synrepo compact` prints plan without mutating, `synrepo compact --apply` prints summary and creates `compact-state.json`

## 7. Graph Integrity Invariant Test

- [x] 7.1 Write a dedicated test that snapshots all graph row counts before compaction, runs `execute_compact`, and asserts all graph row counts are identical afterward
