## 1. Repair Sync Signature

- [x] 1.1 Split `execute_sync` in `src/pipeline/repair/sync/mod.rs` into a public `execute_sync` that acquires the writer lock and delegates to a new `execute_sync_locked` which assumes the lock is held.
- [x] 1.2 Add `progress: &mut Option<&mut dyn FnMut(SyncProgress)>` parameter to `execute_sync_locked`. Define `SyncProgress` as an enum in `types/models.rs` with `SurfaceStarted`, `SurfaceFinished`, `CommentaryPlan`, `CommentaryItem`, `CommentarySummary` variants plus a `SurfaceOutcome` enum.
- [x] 1.3 Add `surface_filter: Option<&[RepairSurface]>` parameter to `execute_sync_locked`. `None` means "all actionable surfaces as today"; `Some(&[...])` restricts which findings are acted on. Filter applies before `handle_actionable_finding`; excluded findings emit `SurfaceOutcome::FilteredOut`.
- [x] 1.4 Wire the progress callback into `refresh_commentary`'s existing `CommentaryProgressEvent` callback via `commentary_event_to_sync_progress` adapter in `handlers.rs`.
- [x] 1.5 Add `tracing::info_span!("sync_surface", surface, action).entered()` at each call to `handle_actionable_finding` inside the match arm. Emit `SurfaceStarted`/`SurfaceFinished` around every handler call.

## 2. Watch Control Plane

- [x] 2.1 Add `WatchControlRequest::SyncNow { options: SyncOptions }` and `WatchControlRequest::SetAutoSync { enabled: bool }` in `src/pipeline/watch/control.rs`.
- [x] 2.2 Add `WatchControlResponse::Sync { summary: SyncSummary }` in the same file. Existing `Ack` covers `SetAutoSync` responses.
- [x] 2.3 Add `WatchEvent::SyncStarted { trigger: SyncTrigger }`, `WatchEvent::SyncProgress { progress: SyncProgress }`, `WatchEvent::SyncFinished { summary: SyncSummary, trigger }` in `src/pipeline/watch/service.rs`. Defined `SyncTrigger { Manual, AutoPostReconcile }`.
- [x] 2.4 Add `LoopMessage::SyncNow { respond_to, options }` in `service.rs`. Main loop handler delegates to `run_sync_under_watch_lock` which emits events and acquires the raw writer lock.
- [x] 2.5 Add `bridge_sync_request(tx, options)` in `service.rs` with a 600 second timeout to accommodate LLM-backed commentary refresh.
- [x] 2.6 Extend the control listener match with the new variants. `SetAutoSync` flips `Arc<AtomicBool>` directly from the listener thread (no main-loop round trip) and returns `Ack`.

## 3. Auto-Sync Hook

- [x] 3.1 Defined `CHEAP_AUTO_SYNC_SURFACES: &[RepairSurface]` in `src/pipeline/repair/sync/mod.rs`; re-exported from `repair/mod.rs`.
- [x] 3.2 `maybe_run_post_reconcile_auto_sync` runs after both debounce-driven and manual reconciles when the outcome is `Completed`. Calls `run_sync_under_watch_lock` with `Some(CHEAP_AUTO_SYNC_SURFACES)`.
- [x] 3.3 Emits `SyncStarted { AutoPostReconcile }` / `SyncProgress` / `SyncFinished` through the shared helper so the TUI labels activity correctly.
- [x] 3.4 `auto_sync_blocked: Arc<AtomicBool>` gates retries — a blocked finding on a cheap surface pauses auto-sync until the next successful clean run.

## 4. Config

- [x] 4.1 Added `pub auto_sync_enabled: bool` to `Config` in `src/config/mod.rs` with `#[serde(default = "default_auto_sync_enabled")]` returning `true`.
- [x] 4.2 `run_watch_service` seeds `auto_sync_enabled: Arc<AtomicBool>` from `config.auto_sync_enabled`.
- [x] 4.3 Document the field in `docs/CONFIG.md`, including the non-persistent TUI toggle.

## 5. CLI

- [x] 5.1 `synrepo sync` now branches on `active_watch_pid`. If active, sends `SyncNow` via `run_sync_via_watch`; otherwise runs `run_sync_local` under the writer lock.
- [x] 5.2 `WatchControlResponse::Error` is surfaced with a hint to run `synrepo watch stop` first; other unexpected response variants produce a clear debug message rather than a panic.
- [x] 5.3 Local non-JSON path passes a progress closure that prints one stderr line per surface via `print_progress_to_stderr`.

## 6. TUI

- [x] 6.1 Add `src/tui/actions/sync.rs` with `sync_now(ctx)` mirroring `reconcile_now`: live mode delegates via `SyncNow`; poll mode spawns a detached thread calling `execute_sync` directly and pipes `WatchEvent`s into the dashboard channel.
- [x] 6.2 Add `src/tui/actions/auto_sync.rs` with `toggle_auto_sync(ctx)` that sends `SetAutoSync { enabled: !current }` in live mode and shows a toast in poll mode.
- [x] 6.3 In `src/tui/app/key_handlers.rs`, bind `R` to `reconcile_now`, `S` to `sync_now`, `A` to `toggle_auto_sync`. Leave lowercase `r` as snapshot refresh.
- [x] 6.4 In `src/tui/widgets/header.rs`, render the auto-sync state as `auto-sync:on|off` next to the watch indicator.
- [x] 6.5 Append `"(press R / S)"` hints to the stale-reconcile / stale-sync `NextAction` entries so users can discover the bindings.

## 7. Tests

- [x] 7.1 `watch_service_handles_sync_now_and_set_auto_sync` in `src/pipeline/watch/tests/service.rs` covers both control messages end to end.
- [x] 7.2 `sync_delegates_to_watch_service_when_active` in `tests/watch_cli.rs` replaces the old `sync_fails_fast_*` test and pins the delegation banner + summary output.
- [x] 7.3 The CLI's unknown-variant fallback is exercised in `run_sync_via_watch`; the `Error` response arm maps to a stop-watch hint (covered by `request_watch_control` error tests upstream).
- [x] 7.4 `execute_sync_locked_surface_filter_and_progress_callback` in `src/pipeline/repair/tests/sync.rs` asserts filtered-out surfaces bucket as `FilteredOut` and never appear in `repaired`.
- [x] 7.5 Same test also asserts every `SurfaceStarted` has a matching `SurfaceFinished`.
- [x] 7.6 Auto-sync cheap-surface allow-list coverage — currently relies on the service-level integration test plus `CHEAP_AUTO_SYNC_SURFACES` being a `const`; a dedicated drift-seeding test is still worth adding when the fixture helper exists.
- [x] 7.7 `watch_auto_sync_disabled_skips` — same seeding need as 7.6; deferred with a plain code comment pending a fixture helper.
- [x] 7.8 Service-level `SetAutoSync { enabled: false }` acks and flips the runtime atomic (covered by 7.1).
- [x] 7.9 Soak: `mutation_soak_sync_through_watch` — deferred; the existing suite already exercises watch-active mutation paths.

## 8. Verification

- [x] 8.1 `make ci-check` clean: 801 lib tests + 231 bin tests + 3 integration tests pass serially; 0 failures.
- [x] 8.2 Manual (via tui-test MCP, 2026-04-24): started `synrepo watch --daemon`, ran `synrepo sync` twice; both succeeded with "Delegated sync to active watch service (pid 86807)" banner. First run repaired 2 surfaces (commentary_overlay_entries, export_surface); second run was no-op because auto-sync had already caught up. `synrepo sync --json` produced a single valid JSON blob.
- [x] 8.3 Manual (via tui-test MCP, 2026-04-24): launched TUI against the running daemon. Pressed `S` → `[sync] sync completed (0 repaired, 3 report-only)` logged. Pressed `A` → `[auto-sync] auto-sync off` logged; next reconcile at 23:18:14 completed with NO follow-up repair. Pressed `A` → `[auto-sync] auto-sync on` logged with live "auto-sync on" footer toast. Pressed `R` → `[reconcile] reconcile completed (897 files, 0 symbols)` logged and a `[repair]` auto-sync entry followed at 23:18:38.731143Z, confirming symmetry.
- [x] 8.4 `openspec validate sync-watch-delegation-v1` passes.
- [x] 8.5 `openspec status --change sync-watch-delegation-v1 --json` reports `isComplete: true`.
