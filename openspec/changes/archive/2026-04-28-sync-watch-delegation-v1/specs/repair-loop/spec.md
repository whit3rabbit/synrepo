## ADDED Requirements

### Requirement: Split sync entry points around the writer lock
`execute_sync` SHALL retain its current signature and behavior (acquire `acquire_write_admission`, then run the repair sync pipeline) so standalone CLI behavior is unchanged. A new `execute_sync_locked` entry point SHALL assume the caller already holds the writer lock and expose optional progress and surface-filter parameters for in-process callers such as the watch service.

#### Scenario: CLI calls sync standalone
- **WHEN** no watch service is active and the user runs `synrepo sync`
- **THEN** the CLI calls the outer `execute_sync`, which acquires the writer lock and delegates to `execute_sync_locked` with both optional parameters set to `None`
- **AND** the resulting behavior and summary match pre-change sync

#### Scenario: Watch service calls sync inline
- **WHEN** the watch service accepts a `sync_now` control request
- **THEN** it acquires the writer lock via `acquire_writer_lock` (the raw lock, bypassing the watch-active admission check)
- **AND** calls `execute_sync_locked` directly with a progress callback that broadcasts `WatchEvent::SyncProgress`

### Requirement: Sync progress callback spans every acted surface
`execute_sync_locked` SHALL accept `progress: Option<&mut dyn FnMut(SyncProgress)>` and emit at least one `SyncProgress` event at the start and end of every surface whose findings it acts on. Commentary refresh progress SHALL be bridged from the existing `CommentaryProgressEvent` callback into `SyncProgress`.

#### Scenario: CLI local sync emits per-surface stderr
- **WHEN** a user runs `synrepo sync` without `--json`
- **THEN** the CLI passes a progress closure that prints one line per surface entry and one line per surface exit to stderr
- **AND** the final summary still prints to stdout, unchanged

#### Scenario: Watch emits sync progress on the event channel
- **WHEN** a watch-delegated sync runs
- **THEN** each surface boundary produces a `WatchEvent::SyncProgress` event on the bounded channel
- **AND** the dashboard can render sync activity in the log pane

### Requirement: Sync surface allow-list filter
`execute_sync_locked` SHALL accept `surface_filter: Option<&[RepairSurface]>`. When `None`, all actionable findings from the repair report SHALL be processed as today. When `Some(allow_list)`, only findings whose `surface` is in `allow_list` SHALL be passed to `handle_actionable_finding`; all other findings SHALL be reported as skipped in the returned `SyncSummary` but not acted upon.

#### Scenario: Watch auto-sync restricts to cheap surfaces
- **WHEN** the watch service runs a post-reconcile auto-sync
- **THEN** it calls `execute_sync_locked` with `surface_filter = Some(&[ExportSurface, RetiredObservations])`
- **AND** commentary refresh is NOT invoked by this call
- **AND** the returned `SyncSummary` reports only the cheap surfaces in `repaired`

### Requirement: Sync tracing spans per surface
`handle_actionable_finding` and adjacent handlers SHALL emit `tracing::info` spans scoped by surface name so that `RUST_LOG=info synrepo sync` produces one span per surface boundary. The span SHALL include the surface and the chosen `RepairAction`.

#### Scenario: Tracing observer sees sync progression
- **WHEN** a user runs `RUST_LOG=info synrepo sync`
- **THEN** the log output contains one span per surface boundary with `surface=<name>` and `action=<action>`
