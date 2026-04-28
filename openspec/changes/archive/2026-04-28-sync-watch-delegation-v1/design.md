## Context

The watch service is the one mutating process that is allowed to hold the writer lock for a repo during watch mode. Every other mutating CLI command currently fails fast when watch is running, except `reconcile`, which has had a `ReconcileNow` control-plane path since watch shipped. The rest of the friction reported by users (no sync while watch runs, no progress while sync runs, no in-TUI sync, constant manual sync after every edit) all stem from not having extended that delegation pattern to the repair sync pipeline, plus missing plumbing on the progress callback that already exists in `refresh_commentary`.

This change closes the asymmetry. It is intentionally conservative: the delegation protocol is request-response (no streaming), the auto-sync allow-list is hard-coded, and the TUI toggle is a live-only flag that does not rewrite config.

## Goals / Non-Goals

**Goals:**

- Let `synrepo sync` succeed while watch is active, with identical CLI output.
- Give users visible per-surface progress during sync from CLI, tracing, and the TUI.
- Run cheap, token-free repairs (export regeneration, compaction) automatically after every drift-producing reconcile when `auto_sync_enabled` is true.
- Bind `R`, `S`, `A` in the TUI dashboard for reconcile, sync, and auto-sync toggle.
- Keep the wire protocol additive and safe under rolling binary upgrades.

**Non-Goals:**

- No auto-refresh of commentary overlay. Token-cost surfaces stay manual.
- No per-surface auto-sync policy config. The allow-list is source-controlled.
- No interval-based sync timer. Post-reconcile is the only auto trigger.
- No streaming progress over the control socket for MVP. Progress goes to the TUI via the event channel; the CLI shows a single final summary when it delegated to watch.
- No extension of this pattern to `export`, `upgrade`, or `links accept` in this change. The pattern this change establishes makes those follow-ups mechanical.

## Decisions

1. **`execute_sync` splits into outer and inner.** The existing public signature (`execute_sync`) keeps `acquire_write_admission` so standalone CLI behavior is unchanged. A new `execute_sync_locked` assumes the caller holds the writer lock and adds `progress: Option<&mut dyn FnMut(SyncProgress)>` and `surface_filter: Option<&[RepairSurface]>` parameters. Watch calls `execute_sync_locked` directly after taking the raw `acquire_writer_lock` (the same path `run_reconcile_pass` already uses).

2. **Request-response protocol, not streaming.** `WatchControlRequest::SyncNow { options }` returns `WatchControlResponse::Sync { summary }` synchronously. The bridge uses a longer timeout (10 minutes) than `ReconcileNow` because sync may invoke LLM-backed commentary refresh. Progress is emitted to the existing `WatchEvent` bounded channel, not streamed back over the socket. This keeps the wire format simple and matches the observation that only the TUI cares about live progress; CLI users in watch-active mode see a single final summary.

3. **Auto-sync allow-list is hard-coded.** `const CHEAP_SURFACES: &[RepairSurface] = &[RepairSurface::ExportSurface, RepairSurface::RetiredObservations]`. Promoting a surface to auto-run is a one-line code change with a visible diff, not a quiet config tweak in someone's repo. This prevents surprising token spend from drift-triggered commentary refresh.

4. **Config flag is a startup seed; TUI toggle is runtime only.** `auto_sync_enabled` in `Config` is read once at watch startup and seeds a runtime `AtomicBool` in the service. `SetAutoSync { enabled }` flips the atomic but does not rewrite `config.toml`. Users who want a persistent change edit the file and restart watch. The TUI toast and `docs/CONFIG.md` make this explicit.

5. **Additive enum extension.** `WatchControlRequest` uses `#[serde(tag = "command", rename_all = "snake_case")]`. Older daemons deserializing `sync_now` or `set_auto_sync` produce a serde error that the existing `Err(error) => WatchControlResponse::Error` arm handles. The CLI treats any `Error` response as a hint to fall back to the old "stop watch first" path.

6. **Writer lock re-entry is not used.** Although `WriterLock` supports same-thread re-entry via its depth counter, the auto-sync hook runs on the watch main-loop thread right after `run_reconcile_pass` releases the lock. It acquires a fresh lock rather than trying to extend the reconcile pass's lock. This keeps the lock-lifetime audit simple.

7. **`R` is shift-R; `r` stays as snapshot-refresh.** Muscle memory matters: lowercase `r` currently maps to the snapshot refresh. Uppercase `R` is the new explicit reconcile, which in live mode delegates over the socket and in poll mode acquires the lock locally. Same pattern for `S` (sync) and `A` (auto-sync toggle).

## Risks / Trade-offs

- **Protocol-mismatch fallback surface.** An older watch daemon running against a newer CLI returns an `Error` response to `SyncNow`. The CLI must map that to a clear "stop watch first" hint instead of panicking or looping. Tested by `cli_sync_falls_back_when_daemon_unknown_variant`.

- **Long sync timeout.** Bridging with a 10-minute timeout means a truly wedged watch loop takes up to ten minutes to surface to the CLI. Mitigation: the TUI user sees no progress events for that interval and can press `A` or stop watch. Alternative considered: no timeout; rejected because it hides bugs in the loop.

- **Auto-sync inside the reconcile loop.** If the reconcile loop is waiting on an auto-sync export regeneration, new filesystem events pile up in the debounce queue. For the cheap-surface allow-list this should be sub-second for typical repos. If export regeneration turns out to take longer than debounce budget in practice, promote the auto-sync call to a separate thread and coordinate via the existing control channel. Covered by the soak test `rapid_file_churn_with_auto_sync_does_not_regress_reconcile_latency`.

- **Default flip for existing users.** `auto_sync_enabled` defaults to `true`. An existing user who upgrades sees export regenerating automatically when they touch code under watch. Mitigation: changelog entry; `A` toggle for one-off opt-out; setting `auto_sync_enabled = false` in `config.toml` for persistent opt-out. Considered defaulting to `false`; rejected because the primary complaint motivating this change is "stop making me run sync manually."

- **TUI poll-mode sync.** Poll-mode `synrepo dashboard` without a watch daemon now runs `execute_sync` on a detached thread when the user presses `S`. If the user quits the TUI while sync is running, the thread continues until completion. Acceptable because the thread holds its own writer lock and will release on drop; worst case the TUI exits before the final toast lands, and the user can check `synrepo status --recent` for the result.
