## Why

`synrepo check` tells users to run `synrepo sync`, but `synrepo sync` hard-rejects whenever the watch service is active. The workaround is to stop watch, run sync, restart watch, which defeats the point of watch and loses the live dashboard state. When sync does run, it emits no per-surface progress until the full summary at the end, so users cannot tell whether a ten-second pause is progress or a hang. The TUI dashboard's `r` key reloads the snapshot view but does not reconcile or sync, and its existing `reconcile_now` action helper is never wired to any keybinding. Finally, every reconcile that surfaces drift forces a manual sync step even for cheap safe repairs (export regeneration, retired-observation compaction) that have no token cost and no human review step.

Reconcile already has a control-socket delegation path (`WatchControlRequest::ReconcileNow`). Extending the same pattern to sync, wiring progress through the existing commentary callback plumbing, and hooking cheap repairs onto the watch reconcile cycle closes the workflow friction end to end.

## What Changes

- Add `WatchControlRequest::SyncNow { options }` and `WatchControlResponse::Sync { summary }`. The watch service acquires the writer lock per request and runs `execute_sync` inline, mirroring `ReconcileNow`.
- Add `WatchControlRequest::SetAutoSync { enabled }` as the live kill switch for Layer 4 auto-sync.
- Split `execute_sync` into `execute_sync` (public, acquires writer lock via `acquire_write_admission`) and `execute_sync_locked` (lock-free inner, callable by watch after it holds the raw writer lock). Thread an optional progress callback and a surface allow-list filter through the inner path.
- Emit `WatchEvent::SyncStarted { trigger }`, `WatchEvent::SyncProgress`, and `WatchEvent::SyncFinished` on the existing bounded channel so the live-mode dashboard can surface sync like it surfaces reconcile today.
- Wire `execute_sync`'s progress callback from the CLI to stderr (per-surface line when not `--json`) and from the watch service into the event channel.
- Add per-surface tracing spans in `src/pipeline/repair/sync/handlers.rs` so `RUST_LOG=info` becomes informative during sync.
- In `src/pipeline/watch/reconcile.rs`, after `run_reconcile_pass` returns, if `auto_sync_enabled` and drift is present, call `execute_sync_locked` with the hard-coded cheap-surface allow-list (`ExportSurface`, `RetiredObservations`). Commentary, proposed cross-links, declared links, and edge drift stay out of the allow-list.
- Add `auto_sync_enabled: bool` to `Config` with a serde default of `true`. Watch reads it at startup and keeps a runtime flag that `SetAutoSync` can flip in-memory.
- Wire TUI keybindings: `R` calls `reconcile_now` (already defined in `src/tui/actions/reconcile.rs`, never bound); `S` calls a new `sync_now`; `A` toggles auto-sync via the control socket in live mode. Preserve lowercase `r` as snapshot refresh. Add an `A:on/off` indicator to the header.

## Capabilities

### New Capabilities

None. All changes extend existing capabilities.

### Modified Capabilities

- `watch-and-ops`: control listener gains `sync_now` and `set_auto_sync` requests; reconcile loop gains a post-reconcile cheap-surface auto-sync step.
- `repair-loop`: `execute_sync` gains optional progress callback and surface-filter parameters, and a lock-free inner entry point that callers holding the writer lock can invoke directly.
- `dashboard`: `R`, `S`, and `A` bindings wire reconcile, sync, and auto-sync toggling into the dashboard in both live and poll modes; header renders `auto-sync:on|off`.

## Impact

- `src/pipeline/watch/control.rs`: new request and response variants.
- `src/pipeline/watch/service.rs`: new `LoopMessage::SyncNow`, new `bridge_sync_request`, new `WatchEvent` variants, post-reconcile auto-sync hook (Layer 4 implementation land).
- `src/pipeline/watch/reconcile.rs`: post-reconcile auto-sync call site.
- `src/pipeline/repair/sync/mod.rs`: `execute_sync` split; progress and filter params threaded.
- `src/pipeline/repair/sync/handlers.rs` and `commentary.rs`: tracing spans, progress callback wire-up.
- `src/bin/cli_support/commands/repair.rs`: `sync` command grows a delegation branch.
- `src/config/mod.rs`: `auto_sync_enabled` field.
- `src/tui/actions/`: new `sync.rs` and `auto_sync.rs` peers; `reconcile.rs` wired in.
- `src/tui/app/key_handlers.rs`, `src/tui/widgets/header.rs`: new bindings and the `A` indicator.
- Docs: `docs/CONFIG.md` documents the config field and its non-persistent TUI toggle.
- Tests: unit coverage for each control message and the auto-sync allow-list; a mutation-soak case for sync-through-watch deadlock detection.

Protocol change is fully additive: older daemons return a deserialization error for unknown variants, so the CLI falls back to the existing "stop watch first" message rather than crashing.
