## 1. Progressive-disclosure doc pass (no new code)

- [x] 1.1 Add `budget` field to `MinimumContextResponse` serialization verification: confirm `budget` key appears in all three budget-tier snapshot outputs (already implemented; verify in existing snapshots)
- [x] 1.2 Update `skill/SKILL.md`: add a "Budget Escalation" section explaining `tiny → normal → deep` as the intended three-step interaction pattern, with a concrete decision rule for when to escalate
- [x] 1.3 Sync cards delta spec into `openspec/specs/cards/spec.md`: merge the progressive-disclosure protocol requirement

## 2. shared `read_recent_activity` function

- [x] 2.1 Create `src/pipeline/recent_activity/mod.rs` with `RecentActivityKind` enum (`Reconcile`, `Repair`, `CrossLink`, `OverlayRefresh`, `Hotspot`), `ActivityEntry` struct (kind, timestamp, payload as `serde_json::Value`), and `RecentActivityQuery` struct (kinds filter, limit, since)
- [x] 2.2 Implement `read_reconcile_event(synrepo_dir)` — reads `reconcile-state.json` via existing `load_reconcile_state`, returns `Option<ActivityEntry>` with `note: "single_entry"` in payload
- [x] 2.3 Implement `read_repair_events(synrepo_dir, limit, since)` — reads `repair-log.jsonl` tail-first (most recent first), deserializes `ResolutionLogEntry`, applies limit/since filter, returns `Vec<ActivityEntry>`
- [x] 2.4 Implement `read_cross_link_events(overlay_db_path, limit, since)` — queries `cross_link_audit` table ordered by `event_at DESC`, returns `Vec<ActivityEntry>`
- [x] 2.5 Implement `read_overlay_refresh_events(overlay_db_path, limit, since)` — queries `commentary` table ordered by `generated_at DESC`, adds `CREATE INDEX IF NOT EXISTS idx_commentary_generated_at` on first use
- [x] 2.6 Implement `read_hotspot_events(repo_root, config, limit)` — builds `GitHistoryIndex`, ranks files by co-change count, returns top-N as `Vec<ActivityEntry>`; returns empty list with `state: "unavailable"` when git is absent
- [x] 2.7 Implement `read_recent_activity(synrepo_dir, repo_root, config, query)` — dispatches to per-kind readers, merges and re-sorts by timestamp, enforces limit cap (max 200, error above); returns `Vec<ActivityEntry>`

## 3. `synrepo status --recent` CLI flag

- [x] 3.1 Add `--recent` flag to the `status` subcommand in `src/bin/cli.rs` (bool flag, default false)
- [x] 3.2 In the status handler, when `--recent` is set call `read_recent_activity` with default limit 20 and all kinds
- [x] 3.3 Add plain-text rendering for `ActivityEntry` list in the status output (below the existing status block)
- [x] 3.4 Ensure `synrepo status --recent --json` includes the activity list as a `recent_activity` key in the JSON output

## 4. `synrepo_recent_activity` MCP tool

- [x] 4.1 Add `RecentActivityParams` struct in `crates/synrepo-mcp/src/main.rs` with `kinds: Option<Vec<String>>`, `limit: Option<usize>` (default 20), `since: Option<String>` (RFC 3339)
- [x] 4.2 Implement `synrepo_recent_activity` MCP handler: parse params, validate limit ≤ 200 (return error if exceeded), call `read_recent_activity`, serialize response as JSON
- [x] 4.3 Register the tool with name `synrepo_recent_activity` and a description that explicitly says it is NOT a session-memory or agent-interaction log

## 5. Tests

- [x] 5.1 Unit test: `read_repair_events` returns entries in reverse-chronological order and respects limit
- [x] 5.2 Unit test: `read_reconcile_event` returns `None` when no reconcile-state file exists and `Some` with `note: "single_entry"` when it does
- [x] 5.3 Unit test: `read_recent_activity` with limit > 200 returns an error
- [x] 5.4 Unit test: `read_recent_activity` with no git repo returns empty hotspot list with `state: "unavailable"`
- [x] 5.5 Unit test: kinds filter includes only the requested event kinds in the result

## 6. Validation

- [x] 6.1 Run `cargo test` and confirm all tests pass
- [x] 6.2 Run `cargo clippy --workspace --all-targets -- -D warnings` and confirm no new warnings
- [x] 6.3 Run `make check` for full CI-equivalent validation
- [x] 6.4 Smoke test: `synrepo status --recent` prints output (even if activity lists are empty) without error
- [x] 6.5 Smoke test: `synrepo-mcp` tool list includes `synrepo_recent_activity`
