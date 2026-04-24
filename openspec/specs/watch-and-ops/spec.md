## Purpose
Define watcher, reconcile, locking, cache lifecycle, storage compatibility, and diagnostics behavior for operating synrepo under normal repository churn.

## Requirements

### Requirement: Define watcher and reconcile behavior
synrepo SHALL define watch-mode behavior, event coalescing, and reconcile passes so repository churn does not silently poison the system.

#### Scenario: Receive a burst of file events
- **WHEN** many filesystem events arrive during a build or refactor
- **THEN** watch behavior coalesces and reconciles them into bounded structural updates
- **AND** stale or missed events are corrected by a defined reconcile path

### Requirement: Define operational lifecycle boundaries
synrepo SHALL define daemon optionality, locking, cache lifecycle, compact behavior, and failure recovery for local operation. Watch mode SHALL be explicit and per-repository: `synrepo watch` runs in the foreground for the current repo, `synrepo watch --daemon` starts the same service detached for the current repo, and standalone CLI behavior remains supported without any daemon.

#### Scenario: Run synrepo with and without a daemon
- **WHEN** a user runs synrepo in standalone or daemon-assisted mode
- **THEN** the ops contract defines the ownership of writes, locks, and snapshots
- **AND** both modes preserve consistent state and recovery expectations

#### Scenario: Start watch explicitly for one repo
- **WHEN** a user runs `synrepo watch` or `synrepo watch --daemon`
- **THEN** synrepo watches only the initialized repository that command targeted
- **AND** the stdio MCP server remains a separate process model rather than the daemon

### Requirement: Define store retention and compatibility operations
synrepo SHALL define retention and maintenance behavior for runtime stores by consuming the storage-compatibility contract, so maintenance behavior is predictable under upgrades and long-lived usage.

#### Scenario: Maintain runtime stores over time
- **WHEN** a user upgrades synrepo or storage exceeds the declared lifecycle boundaries
- **THEN** the ops contract applies the storage-defined store classes and compatibility actions to determine which stores are compacted, migrated, rebuilt, retained, invalidated, or garbage-collected
- **AND** maintenance behavior is observable instead of implicit

### Requirement: Expose operational diagnostics
synrepo SHALL define operational status and diagnostics surfaces sufficient to understand cache state, reconcile health, and failure recovery needs.

#### Scenario: Investigate operational trouble
- **WHEN** a user or agent needs to understand why synrepo is stale or unhealthy
- **THEN** the ops surface provides observable diagnostics
- **AND** the contract avoids treating watcher behavior as opaque background magic

#### Scenario: Enumerate recent reconcile outcomes
- **WHEN** an operator or agent requests the last N reconcile passes
- **THEN** synrepo returns each pass's timestamp, file-count delta, duration, and success/failure sourced from persisted reconcile state
- **AND** the surface does not invent history beyond what is already recorded in `.synrepo/state/reconcile-state.json` and its rotated predecessors

#### Scenario: Enumerate recent repair-log entries
- **WHEN** an operator or agent requests recent repair-log entries
- **THEN** synrepo returns entries read from `.synrepo/state/repair-log.jsonl` with their drift surface, severity, action taken, and timestamp
- **AND** the response is bounded by an explicit limit rather than returning the entire log

### Requirement: Define single-writer operational safety
synrepo SHALL define single-writer safety for daemon and standalone operation, including how concurrent writers are prevented or rejected. A long-lived watch lease SHALL record repo-level watch ownership, while `writer.lock` SHALL remain operation-scoped and guard actual runtime mutations only.

#### Scenario: Run multiple control surfaces at once
- **WHEN** an MCP server, CLI, or background process could write runtime state concurrently
- **THEN** the ops contract defines the authoritative writer and locking behavior
- **AND** state consistency does not depend on undocumented process ordering

#### Scenario: Reconcile while watch service is active
- **WHEN** a user runs `synrepo reconcile` while a watch service already owns the repo
- **THEN** the command delegates `reconcile_now` to the active watch service
- **AND** the reconcile work still acquires the normal write lock inside the watch-owned process

### Requirement: Sequence watch and reconcile after structural graph production exists
synrepo SHALL treat watch and reconcile behavior as downstream of the structural compile that populates the graph automatically, so watcher-driven updates rerun a deterministic producer path instead of inventing a second truth source.

#### Scenario: Plan the first watch-enabled runtime
- **WHEN** a contributor prepares to implement watch and reconcile behavior
- **THEN** the contract assumes an existing structural compile that already rebuilds the relevant graph and substrate state deterministically
- **AND** watcher behavior is defined as a trigger and coalescing layer over that compile path rather than as a separate source of graph facts

### Requirement: Define the first watch-triggered reconcile loop
synrepo SHALL define a watch-triggered update loop that coalesces local filesystem churn into bounded refresh work and uses reconcile passes to correct missed or ambiguous watcher events. The watcher SHALL suppress `.synrepo/` self-events and SHALL remain a trigger layer over the deterministic reconcile path rather than a separate mutation path.

#### Scenario: Handle a burst of local repository changes
- **WHEN** many filesystem events arrive during a build, refactor, or branch switch
- **THEN** synrepo coalesces them into a bounded update cycle instead of naively running one compile per event
- **AND** a defined reconcile path can restore correctness if watcher coverage is incomplete or stale

#### Scenario: Observe runtime-only writes under `.synrepo/`
- **WHEN** the watch service sees filesystem activity only inside `.synrepo/`
- **THEN** it ignores those events for structural refresh purposes
- **AND** it does not trigger reconcile cycles from its own runtime writes

### Requirement: Define initial single-writer runtime safety
synrepo SHALL define the initial single-writer safety model for standalone CLI and future daemon-assisted operation, including lock acquisition, conflict behavior, and recovery expectations.

#### Scenario: Two control surfaces could write runtime state
- **WHEN** multiple local control surfaces attempt to mutate `.synrepo/` state concurrently
- **THEN** the contract defines which writer proceeds, which writer waits or fails, and how the resulting state remains consistent
- **AND** correctness does not depend on undocumented process timing

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

### Requirement: Map watch and daemon state to readiness
Watch and daemon state SHALL appear in the readiness matrix without making the daemon mandatory for normal explicit CLI operation.

#### Scenario: Watch is disabled or stopped
- **WHEN** no watch daemon is active and no foreground watch is running
- **THEN** the readiness matrix marks watch as disabled or stopped
- **AND** the next action names `synrepo watch` or `synrepo watch --daemon` without blocking explicit `sync`, `check`, or card commands

#### Scenario: Watch owner is stale or conflicting
- **WHEN** watch state records a dead owner, live conflicting owner, or unreachable control socket
- **THEN** the readiness matrix marks watch as stale or blocked according to the existing watch-control diagnosis
- **AND** the recommended action follows existing watch cleanup or stop behavior
