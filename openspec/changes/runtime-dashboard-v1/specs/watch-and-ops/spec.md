## ADDED Requirements

### Requirement: Expose watch and reconcile as operator actions
synrepo SHALL expose start-watch, stop-watch, and reconcile-now as named operator actions callable by an interactive operator surface (the dashboard) in addition to the existing CLI subcommands. These actions SHALL delegate to the existing watch-control and reconcile code paths and SHALL NOT introduce a parallel mutation path.

#### Scenario: Dashboard requests start watch
- **WHEN** the dashboard invokes the start-watch action for the current repository
- **THEN** the action starts the watch service via the same code path used by `synrepo watch` or `synrepo watch --daemon`
- **AND** the existing watch lease and writer-lock semantics are preserved

#### Scenario: Dashboard requests reconcile now while watch is active
- **WHEN** the dashboard invokes reconcile-now while a watch service already owns the repository
- **THEN** the action delegates `reconcile_now` to the active watch service rather than running a second writer
- **AND** the resulting pass records its outcome in `.synrepo/state/reconcile-state.json`

#### Scenario: Dashboard requests stop watch
- **WHEN** the dashboard invokes the stop-watch action
- **THEN** the action stops the active watch service via the same code path used by `synrepo watch stop`
- **AND** stale lease or socket artifacts left behind are surfaced rather than silently ignored

### Requirement: Foreground watch may host the interactive dashboard
synrepo SHALL define that `synrepo watch` in the foreground (no `--daemon`) MAY host the interactive dashboard in-process when stdout is a TTY, using the same watch-control code path as prior foreground-watch behavior. An explicit opt-out flag (`--no-ui`, `--log`, or equivalent) SHALL force plain log-line output for scripts and CI. Non-TTY stdout SHALL behave as if the opt-out flag was set.

#### Scenario: Foreground watch in a TTY hosts the dashboard
- **WHEN** a user runs `synrepo watch` in the foreground in a TTY without the opt-out flag
- **THEN** the watch service runs in-process and the interactive dashboard is displayed in live mode
- **AND** the watch-service code path is unchanged from prior foreground-watch behavior

#### Scenario: Foreground watch with opt-out prints plain logs
- **WHEN** a user runs `synrepo watch --no-ui` (or the equivalent log-only flag)
- **THEN** the watch service runs in the foreground and emits plain log lines
- **AND** no TUI alternate screen is entered

#### Scenario: Foreground watch with non-TTY stdout prints plain logs
- **WHEN** a user runs `synrepo watch` without `--daemon` and stdout is not a TTY (pipe, redirect, CI)
- **THEN** the watch service runs with plain log-line output
- **AND** previously-scripted invocations such as `synrepo watch > watch.log` continue to work

### Requirement: Surface watch-ownership conflicts to interactive callers
synrepo SHALL define how watch-ownership or writer-lock conflicts are surfaced to interactive callers (the dashboard) without panicking the caller or bypassing locks. The conflict SHALL be reported as a structured event that identifies the owner PID and the lease or lock state, so the interactive caller can display it and remain usable.

#### Scenario: Reconcile-now blocked by foreign writer
- **WHEN** reconcile-now is requested while another process holds the writer lock and no watch service is active to delegate to
- **THEN** the action returns a structured conflict event naming the holder PID and lock-acquisition timestamp
- **AND** the interactive caller remains responsive and can retry or cancel

#### Scenario: Start-watch blocked by existing lease
- **WHEN** start-watch is requested while a watch lease already exists for a live PID
- **THEN** the action returns a structured conflict event naming the current owner PID
- **AND** the action does not forcibly take the lease

## MODIFIED Requirements

### Requirement: Expose initial reconcile and runtime diagnostics
synrepo SHALL expose a small operational diagnostics surface sufficient to understand stale state, recent reconcile outcomes, lock conflicts, maintenance needs, and watch-service ownership for the current runtime. The diagnostics SHALL be available as a shared structured status snapshot consumed by both `synrepo status` and the interactive dashboard, so CLI output and dashboard output do not drift.

#### Scenario: Investigate why synrepo appears stale
- **WHEN** a user or agent needs to understand whether watch and reconcile behavior is healthy
- **THEN** synrepo provides observable diagnostics about reconcile health, writer ownership, or stale runtime state
- **AND** the operator does not have to infer system state from silent background behavior

#### Scenario: Inspect active watch ownership
- **WHEN** a user runs `synrepo watch status` or `synrepo status` while watch mode is active
- **THEN** synrepo reports the watch mode, owner PID, and recent reconcile outcome
- **AND** stale lease or socket artifacts are surfaced explicitly rather than silently ignored

#### Scenario: Request a bounded recent-activity view
- **WHEN** an operator runs `synrepo status --recent` or an agent invokes `synrepo_recent_activity`
- **THEN** synrepo returns a bounded enumeration of recent reconcile, repair, cross-link, and overlay-refresh events drawn from already-persisted state
- **AND** the surface refuses unbounded lookback and does not record caller identity, prompt content, or agent-facing interactions

#### Scenario: Dashboard and CLI consume a shared snapshot
- **WHEN** the dashboard or `synrepo status` renders diagnostics
- **THEN** both render from the same structured status snapshot
- **AND** a field added to the snapshot is visible in both surfaces without duplicated computation
