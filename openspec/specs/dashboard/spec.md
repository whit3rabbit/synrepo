## Purpose
Define the interactive dashboard, smart entry experience, guided setup and repair wizards, agent-integration completion flow, and theming contract for synrepo's interactive operator surface.
## Requirements
### Requirement: Provide a smart no-subcommand entry experience
synrepo SHALL define a smart entry experience so that invoking the binary with no subcommand runs the runtime probe and routes the user to the dashboard, setup wizard, or repair wizard. All existing explicit subcommands SHALL continue to work unchanged, and an explicit `synrepo dashboard` subcommand SHALL be available.

#### Scenario: Bare invocation on a ready repository
- **WHEN** a user runs `synrepo` in a repository the probe classifies as `ready`
- **THEN** the dashboard opens
- **AND** existing subcommands such as `synrepo status` behave exactly as before

#### Scenario: Bare invocation on an uninitialized repository
- **WHEN** a user runs `synrepo` in a directory the probe classifies as `uninitialized`
- **THEN** the setup wizard opens

#### Scenario: Bare invocation on a partial repository
- **WHEN** a user runs `synrepo` in a directory the probe classifies as `partial`
- **THEN** the repair wizard opens with the structured list of missing components
- **AND** existing `.synrepo/` state is preserved

#### Scenario: Explicit dashboard alias
- **WHEN** a user runs `synrepo dashboard`
- **THEN** the command runs the runtime probe and opens the dashboard on `ready` state
- **AND** on non-ready state the command exits non-zero with a message directing the user to `synrepo` (bare) or the corresponding wizard command

#### Scenario: Non-TTY fallback
- **WHEN** bare `synrepo` is invoked and stdout is not a TTY or `--no-color` is set and the terminal cannot host a TUI
- **THEN** the binary prints a concise text status summary and exits zero
- **AND** it does not enter the TUI alternate screen

### Requirement: Dashboard supports live and poll modes
synrepo's dashboard SHALL support two modes. In **poll mode** (entered via bare `synrepo` or `synrepo dashboard`) the dashboard does not host a watch service in-process and periodically refreshes the shared status snapshot. In **live mode** (entered via `synrepo watch` in the foreground, without `--daemon` and without an explicit opt-out flag) the dashboard hosts the watch service in-process and streams its events into the log pane in real time.

#### Scenario: Bare invocation opens poll mode
- **WHEN** a user runs bare `synrepo` or `synrepo dashboard` on a ready repository
- **THEN** the dashboard opens in poll mode
- **AND** no watch service is started in this process

#### Scenario: Foreground watch opens live mode
- **WHEN** a user runs `synrepo watch` in a TTY without `--daemon` and without an explicit opt-out of the TUI
- **THEN** the dashboard opens in live mode
- **AND** the watch service runs in-process in the same code path as prior foreground-watch behavior
- **AND** watch events stream into the dashboard log pane

#### Scenario: Stop-watch in live mode exits the app
- **WHEN** a user invokes the stop-watch action while the dashboard is in live mode
- **THEN** the watch service stops and the dashboard process exits
- **AND** the dashboard does not silently transition to poll mode while claiming a watch is running

#### Scenario: Non-TTY foreground watch falls back to plain logs
- **WHEN** `synrepo watch` is invoked without `--daemon` but stdout is not a TTY (pipe, redirect, CI)
- **THEN** the watch service runs with plain log-line output and does not enter the TUI alternate screen

### Requirement: Define the dashboard operator surface
synrepo SHALL define a terminal dashboard that presents operational state for a single repository in a dark theme and provides quick actions that reuse existing control-plane primitives.

#### Scenario: Header surfaces operational identity
- **WHEN** the dashboard opens
- **THEN** the header displays the repo path, mode (`auto` or `curated`), watch state, reconcile health, writer-lock state, and MCP readiness

#### Scenario: Main panes surface actionable state
- **WHEN** the dashboard opens
- **THEN** the layout includes panes for system health, recent activity, next actions or handoffs, quick actions, and an event or notification log

#### Scenario: Suggestion tab surfaces large-file refactor candidates
- **WHEN** the dashboard opens on a ready repository
- **THEN** a `Suggestion` tab is available after the `MCP` tab as key `8`
- **AND** the tab lists non-test source files over 300 physical lines with line count, language, path, symbol count, and modularity hint
- **AND** suggestion rows are loaded lazily when the tab is selected and refreshed by pressing `r` while the tab is active

#### Scenario: Quick actions reuse control-plane primitives
- **WHEN** the user invokes a quick action for start watch, stop watch, reconcile now, refresh, open setup or repair flow, or open agent-integration flow
- **THEN** the action dispatches to the existing watch-control, reconcile, and setup primitives
- **AND** the action acquires the writer lock through the existing mechanism rather than bypassing it

#### Scenario: Lock conflict surfaced, not swallowed
- **WHEN** a quick action fails to acquire the writer lock or encounters a watch-ownership conflict
- **THEN** the log pane records the conflict with a structured entry naming the owner PID and lease state
- **AND** the dashboard remains usable

### Requirement: Dashboard consumes the shared status snapshot
synrepo SHALL define a structured status snapshot consumed by both the `synrepo status` renderer and the dashboard, so the dashboard is a viewer over already-computed state and does not duplicate status logic.

#### Scenario: Single source of truth
- **WHEN** the dashboard renders health, activity, reconcile, context export freshness, overlay cost, and writer-lock state
- **THEN** it reads from the same snapshot struct used by the `synrepo status` formatter
- **AND** adding a new field to the snapshot surfaces it in both renderers

### Requirement: Dashboard MUST NOT host the stdio MCP server
synrepo's dashboard process SHALL NOT launch the stdio MCP server in-process. The dashboard SHALL only report MCP readiness and surface registration instructions for external agent clients.

#### Scenario: MCP readiness reporting
- **WHEN** the dashboard is open
- **THEN** the header reports whether `synrepo mcp --repo <path>` registration is present for the configured agent target
- **AND** the dashboard does not bind stdin or stdout to an MCP transport while it is rendering

### Requirement: Provide a guided setup wizard
synrepo SHALL provide a guided setup wizard that runs when the runtime probe classifies the repo as `uninitialized`. The wizard SHALL follow a defined arc (splash, mode, agent target, optional embeddings setup, optional explain setup, confirm, run, land-in-dashboard), SHALL call decomposed setup steps (initialize runtime, write agent shim, register MCP, apply integration, apply embeddings config when selected, apply explain config when selected), and SHALL NOT write any file-system state before the explicit "Confirm + run" step. Bare `synrepo` on a fresh repo and interactive no-flag `synrepo init` on a fresh repo SHALL enter the wizard directly without an intermediate "Do you want to set up?" confirmation.

#### Scenario: Enter the wizard from bare invocation on a fresh repo
- **WHEN** a user runs bare `synrepo` in a directory the probe classifies as `uninitialized` and stdout is a TTY
- **THEN** the wizard opens directly at the splash step
- **AND** the user is not shown a separate "Do you want to set up synrepo?" prompt before the wizard begins

#### Scenario: Enter the wizard from no-flag init on a fresh repo
- **WHEN** a user runs `synrepo init` with no flags in a directory the probe classifies as `uninitialized` and stdout is a TTY
- **THEN** the same guided setup wizard opens
- **AND** flagged or non-TTY init invocations keep the low-level bootstrap behavior

#### Scenario: Complete a fresh setup
- **WHEN** the wizard opens on an uninitialized repository
- **THEN** the user proceeds through splash, mode selection, agent-target selection (including a first-class "Skip" option), optional embeddings setup, optional explain setup, and a plan-confirmation step before any writes occur
- **AND** the wizard applies selected embeddings and explain config before the initial runtime build, runs init, writes the chosen shim if any, registers MCP for the chosen target if any, persists selected explain credentials or endpoints, performs a first reconcile, records the project, and transitions to the dashboard with a one-shot welcome message

#### Scenario: Cancel setup before any writes
- **WHEN** the user cancels the setup wizard at or before the "Confirm + run" step
- **THEN** no `.synrepo/` content is created
- **AND** the binary exits zero

#### Scenario: Cancel setup mid-run
- **WHEN** the user cancels the setup wizard after confirm while a step is executing
- **THEN** any partial on-disk state is left in place for later recovery
- **AND** the next bare `synrepo` invocation routes to the repair wizard based on the probe classification

#### Scenario: Pre-select agent target from observational signals
- **WHEN** the wizard reaches the agent-target step in a repository (or home directory) that contains hints for one or more supported targets
- **THEN** the wizard pre-highlights the first detected target in detection order
- **AND** the user can override the pre-selection or choose "Skip"

### Requirement: Non-TTY first-run must not prompt
synrepo SHALL NOT enter the interactive setup wizard when bare `synrepo` is invoked on an uninitialized or partial repository and stdout is not a TTY. The binary SHALL instead print a short message naming the explicit subcommand to run and exit non-zero.

#### Scenario: Bare invocation on a fresh repo with piped output
- **WHEN** a user runs `synrepo | tee log` in a directory the probe classifies as `uninitialized`
- **THEN** the binary prints a message directing the user to run bare `synrepo` in a TTY for guided setup, `synrepo setup <tool>` for scripted setup, or `synrepo init --mode auto` for runtime-only bootstrap
- **AND** it exits non-zero without entering the TUI alternate screen

#### Scenario: Bare invocation on a partial repo with piped output
- **WHEN** a user runs bare `synrepo` in a directory the probe classifies as `partial` and stdout is not a TTY
- **THEN** the binary prints a short summary of the missing components and a pointer to `synrepo status` or `synrepo upgrade`
- **AND** it exits non-zero

### Requirement: Provide a guided repair wizard
synrepo SHALL provide a guided repair wizard that runs when the runtime probe classifies the repo as `partial`. The wizard SHALL list exactly the missing or blocked components, SHALL repair them in place, and SHALL NOT destroy or reinitialize existing `.synrepo/` state without explicit user confirmation. The repair wizard SHALL NOT re-prompt for mode selection or replay the first-run splash; it SHALL jump directly to the missing-components list.

#### Scenario: Repair a missing config
- **WHEN** the wizard opens on a repo where `.synrepo/` exists but `config.toml` is missing
- **THEN** the wizard offers to write a default config and run a reconcile
- **AND** accepting the offer leaves existing store data intact

#### Scenario: Repair a compat-blocked store
- **WHEN** the wizard opens on a repo where the storage compatibility evaluation requires action (rebuild, migrate, or invalidate)
- **THEN** the wizard surfaces the compatibility plan and defers to the existing `synrepo upgrade --apply` flow
- **AND** it does not invoke any action without explicit user confirmation

#### Scenario: Complete repair and transition
- **WHEN** all listed components have been repaired
- **THEN** the wizard re-runs the runtime probe
- **AND** on `ready` classification it transitions to the dashboard

### Requirement: Provide an agent-integration completion flow
synrepo SHALL surface an agent-integration completion flow when the runtime probe classifies runtime as `ready` but agent integration as `absent` or `partial`. The flow SHALL NOT block core repo operation and SHALL be launchable from the dashboard.

#### Scenario: Ready repo with no agent shim
- **WHEN** the dashboard opens and no shim exists for any supported target
- **THEN** the dashboard header shows agent integration as incomplete
- **AND** a quick action opens the integration flow that offers to write a shim and register MCP for the selected target

#### Scenario: Ready repo with shim but no MCP registration
- **WHEN** the dashboard opens and a shim exists but MCP registration is missing
- **THEN** the integration flow offers to complete MCP registration only
- **AND** the existing shim file is not overwritten

### Requirement: Provide explain setup from the dashboard
synrepo SHALL surface explain setup from a ready repository dashboard without requiring the user to exit and run a separate setup command.

#### Scenario: Ready repo configures explain in place
- **WHEN** the dashboard is open on a ready repository
- **THEN** a quick action labeled `configure optional explain` is available
- **AND** activating it opens the explain setup sub-wizard, applies the selected config, and returns to the dashboard

#### Scenario: Explain tab setup alias remains available
- **WHEN** the user is on the Explain tab
- **THEN** pressing `s` opens the same explain setup sub-wizard

### Requirement: Ship one built-in dark theme with a plain-terminal escape
synrepo's dashboard SHALL ship exactly one built-in dark palette in v1, with a single `--no-color` escape that disables all styling for plain or non-ANSI terminals.

#### Scenario: Default rendering
- **WHEN** the dashboard is opened in a color-capable terminal without `--no-color`
- **THEN** the dark palette is applied with distinct colors for healthy, stale or warning, blocked or error, active watch, and agent or MCP accents

#### Scenario: Plain terminal rendering
- **WHEN** the dashboard is opened with `--no-color` or in a terminal that does not support ANSI color
- **THEN** all output renders without color codes
- **AND** semantic distinctions (healthy vs. stale vs. blocked) are preserved via text labels or glyphs

### Requirement: Surface context metrics in operator views
The shared status snapshot SHALL include context metrics so `synrepo status --json` and the dashboard can report card usage and context savings without duplicating logic. The dashboard Health tab SHALL render, at minimum: cards served, average tokens per card, tokens avoided (estimated raw-file tokens saved), and stale responses. The stale-responses row SHALL use the `Stale` severity when the counter is greater than zero so operators notice accumulating advisory staleness without reading the full JSON snapshot.

#### Scenario: Dashboard renders context metrics
- **WHEN** the dashboard opens on a ready repository with non-empty context metrics
- **THEN** the Health tab displays rows for cards served, average card tokens, tokens avoided, and stale responses
- **AND** the stale-responses row is elevated to `Stale` severity when the count is greater than zero

#### Scenario: Context metrics absent
- **WHEN** no context metrics have been recorded yet
- **THEN** the context, tokens-avoided, and stale-responses rows are omitted rather than rendered as zero
- **AND** the remaining Health rows render unchanged

### Requirement: Render capability readiness states
The dashboard SHALL render capability readiness rows from the shared matrix rather than running independent subsystem checks.

#### Scenario: Readiness matrix contains degraded rows
- **WHEN** the dashboard receives readiness rows for parser failures, stale index, missing git, disabled embeddings, disabled watch, or unavailable overlay
- **THEN** it displays each row with state, severity, and next action
- **AND** the dashboard preserves the distinction between disabled, unavailable, degraded, stale, and blocked

### Requirement: Provide a trust-focused dashboard view
The dashboard SHALL provide a trust-focused view that reports context quality, advisory overlay health, degraded surfaces, and bounded current-change impact without duplicating status or repair scan logic.

#### Scenario: Dashboard renders trust signals
- **WHEN** the dashboard opens on a ready repository with context metrics or overlay-note data
- **THEN** it exposes cards served, average card tokens, estimated tokens avoided, stale responses, truncation or escalation counts, and overlay-note lifecycle counts
- **AND** each row is sourced from the shared status snapshot, context metrics, repair report, recent activity, or overlay-note aggregate data

#### Scenario: Trust data has not been recorded
- **WHEN** no context metrics or overlay-note aggregates exist yet
- **THEN** the trust view labels the relevant group as no data
- **AND** it does not render no-data as a proven zero-count healthy state

### Requirement: Surface bounded current-change impact
The dashboard SHALL provide a bounded current-change impact summary when changed-file, symbol, test, or risk data is available.

#### Scenario: Current change data is available
- **WHEN** the status snapshot or bounded query layer can identify changed files with affected symbols, linked tests, or open risks
- **THEN** the trust view displays a capped summary of those items
- **AND** the summary labels unavailable data sources instead of silently omitting them

### Requirement: Provide a doctor aggregation command
synrepo SHALL provide a `synrepo doctor` command that consumes the shared status snapshot and reports only components whose severity is not `Healthy`. The command SHALL exit zero when every component is healthy and non-zero when any component is stale or blocked.

#### Scenario: Healthy repo exits zero
- **WHEN** an operator runs `synrepo doctor` on a repository whose status snapshot reports every component as `Healthy`
- **THEN** the command prints a short confirmation line
- **AND** the command exits with status 0

#### Scenario: Degraded repo exits non-zero
- **WHEN** an operator runs `synrepo doctor` on a repository whose status snapshot reports at least one stale or blocked component
- **THEN** the command prints a compact list naming each degraded component, its severity, and the recommended action
- **AND** the command exits with a non-zero status

#### Scenario: JSON output for CI
- **WHEN** an operator runs `synrepo doctor --json`
- **THEN** the command prints a structured JSON report of degraded components
- **AND** on a healthy repository the JSON report contains an empty degraded list and the command still exits zero

#### Scenario: Single source of truth with status
- **WHEN** a new field is added to the shared status snapshot
- **THEN** `synrepo doctor` surfaces it via the same severity-based filter used by the dashboard without duplicating status logic

### Requirement: Dashboard binds reconcile, sync, and auto-sync keys
The dashboard SHALL bind uppercase `R` to reconcile-now, uppercase `S` to sync-now, and uppercase `A` to auto-sync toggle. Lowercase `r` SHALL continue to refresh the status snapshot only. In live mode these keys SHALL delegate via the watch control socket; in poll mode `R` and `S` SHALL spawn a detached worker thread that takes the writer lock and runs the operation locally, while `A` SHALL show a toast indicating that auto-sync requires an active watch service.

#### Scenario: User presses R in live mode
- **WHEN** the dashboard is running with a live watch service and the user presses `R`
- **THEN** the dashboard sends `reconcile_now` over the control socket
- **AND** surfaces the returned outcome in the log pane

#### Scenario: User presses S in poll mode
- **WHEN** the dashboard is running without a watch service and the user presses `S`
- **THEN** the dashboard spawns a detached thread that runs `execute_sync` locally
- **AND** the existing activity spinner lights while the thread runs
- **AND** a completion toast displays the repaired/blocked counts

#### Scenario: User presses A
- **WHEN** the dashboard is running with a live watch service and the user presses `A`
- **THEN** the dashboard sends `set_auto_sync { enabled: !current }` and updates the header indicator on ack

### Requirement: Dashboard exposes persistent worktree discovery toggle
The dashboard Actions tab SHALL expose uppercase `W` as a confirmed toggle for the repo-local `include_worktrees` config field. `include_worktrees` SHALL remain enabled by default. The toggle SHALL persist to `.synrepo/config.toml`, refresh the dashboard snapshot, and tell the operator to run reconcile; it SHALL NOT run reconcile automatically.

#### Scenario: User disables linked worktrees
- **WHEN** the dashboard is opened for an initialized repo with `include_worktrees = true`
- **AND** the user presses `W` and confirms
- **THEN** `.synrepo/config.toml` contains `include_worktrees = false`
- **AND** the dashboard logs the action and refreshes the quick-action label

### Requirement: Dashboard header shows auto-sync state
The dashboard header SHALL render the current auto-sync state as `auto-sync:on` or `auto-sync:off` next to the watch indicator when watch is active, and SHALL render `auto-sync:inactive` (or omit the segment) when watch is not active.

#### Scenario: Header renders live state
- **WHEN** the dashboard is attached to a live watch service with `auto_sync_enabled = true`
- **THEN** the header includes `auto-sync:on`

#### Scenario: Header reflects a runtime toggle
- **WHEN** the user presses `A` and the service acks
- **THEN** the header's auto-sync segment flips to `auto-sync:off` on the next frame

### Requirement: Stale-surface next-action hints reference the new bindings
The dashboard's `NextAction` rendering SHALL, when a stale surface is present that `R` or `S` can repair, include a short `(press R)` or `(press S)` hint next to the action label so users discover the bindings.

#### Scenario: Stale reconcile with fresh bindings
- **WHEN** the snapshot reports reconcile is stale and the dashboard has an `R` binding
- **THEN** the rendered next-action line includes `(press R)` next to the reconcile suggestion

### Requirement: Provide a global project shell
The dashboard SHALL support a global shell over the managed project registry. The shell SHALL own the project list, an active project ID, and project-scoped runtime states. Rendering and actions SHALL operate on exactly one active project at a time.

#### Scenario: Open dashboard inside a registered project
- **WHEN** the user runs bare `synrepo` from inside a registered initialized project
- **THEN** the dashboard opens with that project active
- **AND** the header shows the project display name and path

#### Scenario: Open dashboard outside a project
- **WHEN** the user runs bare `synrepo` from outside an initialized project and the registry contains managed projects
- **THEN** the dashboard opens the project picker
- **AND** no repository action runs until the user selects a project

#### Scenario: Explicit repo preserves single-project behavior
- **WHEN** the user provides `--repo`
- **THEN** synrepo probes and routes that repository as an explicit target
- **AND** registry project selection does not override the explicit repository

### Requirement: Switch projects atomically
The dashboard SHALL switch the active project by loading or reusing a project-scoped runtime state and clearing transient global/project modals that cannot safely survive the switch.

#### Scenario: Switch clears transient actions
- **WHEN** the user switches from project A to project B while a confirm modal, folder picker, or pending explain run is active
- **THEN** those transient states are cleared
- **AND** project B renders from its own snapshot, log, materializer, explain preview, and watch state

#### Scenario: Cached state stays project-scoped
- **WHEN** the user switches away from a project and later switches back
- **THEN** project-scoped cached preview, materializer state, log, and scroll state belong only to that project
- **AND** no cached state from another project is displayed

### Requirement: Provide fast project selection
The dashboard SHALL provide a project picker opened by `[p]`. The picker SHALL list registered projects sorted by recent use, support filtering, and allow switching, renaming, adding the current directory, and detaching a selected project.

#### Scenario: Picker shows project status
- **WHEN** the project picker is open
- **THEN** each row shows display name, path, derived health, watch state, lock state, and integration state when available

#### Scenario: Detach from picker is non-destructive
- **WHEN** the user detaches a selected project from the picker
- **THEN** the registry entry is removed
- **AND** repository-local state is left untouched

### Requirement: Provide persistent Integrations and Repos tabs
The dashboard SHALL order tabs as `[1] Repos`, `[2] Live`, `[3] Health`, `[4] Actions`, `[5] Explain`, `[6] Integrations`, `[7] Suggestion`, and `[8] Trust`. The Integrations tab SHALL report active-project MCP enablement across known agent targets, including scope, source trigger, and config path. The Repos tab SHALL list registry-managed projects only and allow switching the active project.

#### Scenario: Integrations tab reports active project registration
- **WHEN** the dashboard renders the Integrations tab for an active project
- **THEN** each row shows agent, status, scope, trigger/source, and config path when known
- **AND** the dashboard does not launch or host the MCP stdio server

#### Scenario: Integrations tab installs repo-local MCP
- **WHEN** the user presses `i` on the Integrations tab and confirms a target
- **THEN** synrepo writes or preserves that target's project skill or instruction through agent-config
- **AND** synrepo registers that target's MCP config in project scope through agent-config
- **AND** the registered server command is `synrepo mcp --repo .`
- **AND** pressing `i` outside the Integrations tab still launches the generic integration wizard

#### Scenario: Repos tab switches projects
- **WHEN** the dashboard renders the Repos tab
- **THEN** rows come from the managed-project registry with health, watch, lock, integration, and path
- **AND** Enter switches to the selected project without scanning unrelated filesystem paths

### Requirement: Scope dashboard actions to projects
Every dashboard action that reads or mutates repository-scoped synrepo state SHALL be dispatched with an explicit project context containing project ID, display name, repo root, and `.synrepo/` path.

#### Scenario: Action after project switch
- **WHEN** the user switches to project B and invokes reconcile, sync, watch, materialize, explain, docs export, docs clean, or auto-sync
- **THEN** the action runs against project B
- **AND** logs identify the project when ambiguity is possible

### Requirement: Show all-project watch visibility
The dashboard SHALL expose watch status for every registered project without starting or stopping background project watchers implicitly.

#### Scenario: Watch manager lists all projects
- **WHEN** the user opens the project picker or watch manager
- **THEN** every registered project row shows watch running, inactive, stale, corrupt, or missing-path status
- **AND** start/stop actions apply only to the selected project

### Requirement: Preserve essential footer hints
The dashboard footer SHALL keep essential global hints visible even when a toast is active.

#### Scenario: Toast with essentials
- **WHEN** a toast is visible
- **THEN** the footer still shows at least project picker, help, and quit hints when width permits

### Requirement: Confirm heavyweight or destructive actions
The dashboard SHALL require confirmation before applying destructive or expensive operations from quick actions or command palette entries.

#### Scenario: Materialize confirmation
- **WHEN** the user requests graph materialization from the dashboard
- **THEN** the dashboard opens a confirmation dialog before starting the operation

#### Scenario: Docs clean preview applies from modal
- **WHEN** the user previews cleaning materialized docs
- **THEN** the dashboard shows the preview result
- **AND** applying deletion requires an explicit confirmation from that preview state

### Requirement: Provide accessible dashboard rendering
The dashboard SHALL support no-color, reduced-motion, and ASCII-only rendering modes. Semantic status SHALL NOT rely on color, glyph shape, or bold styling as the only signal.

#### Scenario: Reduced motion removes spinner animation
- **WHEN** reduced-motion or ASCII-only mode is active and a reconcile is running
- **THEN** the header renders a textual running marker instead of animated Braille frames

#### Scenario: Plain severity labels
- **WHEN** no-color or ASCII-only mode is active
- **THEN** healthy, warning, and blocked states include textual prefixes such as `[ok]`, `[warn]`, or `[blocked]`

### Requirement: Use viewport-aware live pagination
The Live tab SHALL base page-up and page-down movement on the last rendered visible row count rather than a fixed constant.

#### Scenario: Page movement adapts to viewport
- **WHEN** the Live tab is rendered in a small or large terminal
- **THEN** PageUp and PageDown move by the visible row count minus headroom
- **AND** movement is clamped to at least one row
