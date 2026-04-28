## ADDED Requirements

### Requirement: Watch control delegates sync without stopping
The watch control listener SHALL accept a `sync_now` request while holding the watch lease and execute one repair sync pass inline, mirroring how `reconcile_now` delegates reconciles today.

#### Scenario: CLI sync with watch active
- **WHEN** a user runs `synrepo sync` while `synrepo watch --daemon` is active for the same repo
- **THEN** the CLI sends a `sync_now` control request over the per-repo socket
- **AND** the watch service acquires the writer lock, runs `execute_sync_locked` against the graph and overlay stores, and responds with a `Sync { summary }` message
- **AND** the CLI prints the resulting summary with the same text or JSON formatting it uses for standalone sync
- **AND** the user does not have to stop watch

#### Scenario: Older daemon receives unknown variant
- **WHEN** a newer `synrepo` CLI sends `sync_now` to an older watch daemon that does not recognize the variant
- **THEN** the daemon returns a deserialization `Error` response
- **AND** the CLI surfaces a clear "stop watch first" hint instead of crashing or looping

### Requirement: Auto-sync runs cheap safe surfaces after reconcile
The watch service SHALL, when `auto_sync_enabled` is true, run `execute_sync_locked` for a hard-coded allow-list of cheap surfaces after every reconcile pass that completes with any drift or observed changes. The allow-list is exactly `ExportSurface` and `RetiredObservations`. Other surfaces (commentary, proposed cross-links, declared links, edge drift) SHALL NOT auto-run.

#### Scenario: Drift touches an auto-sync surface
- **WHEN** a reconcile pass completes and leaves the export epoch stale
- **THEN** the watch service runs a targeted sync that regenerates the export surface
- **AND** emits `sync_started { trigger: auto_post_reconcile }` and `sync_finished` events so the live dashboard can label the activity
- **AND** commentary refresh is not invoked by this auto-sync

#### Scenario: Auto-sync blocked on a cheap surface
- **WHEN** a prior auto-sync emitted a blocked finding for a cheap surface
- **THEN** the next reconcile pass SHALL NOT retry the auto-sync for that surface until a subsequent reconcile succeeds first

### Requirement: Runtime toggle for auto-sync
The watch control listener SHALL accept `set_auto_sync { enabled }` requests and flip a runtime flag owned by the service process. This flag SHALL NOT write to `config.toml`. Restarting the service re-reads the on-disk `auto_sync_enabled` config value.

#### Scenario: Dashboard user flips the toggle
- **WHEN** the user presses `A` in the live dashboard while watch is running
- **THEN** the dashboard sends `set_auto_sync { enabled: !current }` over the socket
- **AND** the watch service acknowledges with an `Ack` response
- **AND** subsequent reconciles honor the new runtime value
- **AND** `config.toml` remains unchanged on disk

### Requirement: Watch events label sync activity
The watch service SHALL emit `sync_started { trigger }`, `sync_progress`, and `sync_finished` events on the existing bounded `WatchEvent` channel so observers (notably the live-mode dashboard) can display sync activity. `SyncTrigger` SHALL distinguish `manual` (user-requested via `sync_now`) from `auto_post_reconcile`.

#### Scenario: Dashboard observes a delegated sync
- **WHEN** a `sync_now` request arrives and executes
- **THEN** subscribers see `sync_started { trigger: manual }`, zero or more `sync_progress` events, and one `sync_finished { trigger: manual }`
- **AND** the dashboard's existing activity spinner lights during the run
