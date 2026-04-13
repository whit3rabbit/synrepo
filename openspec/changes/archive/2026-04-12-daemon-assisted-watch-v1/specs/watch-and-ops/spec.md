## MODIFIED Requirements

### Requirement: Define operational lifecycle boundaries
synrepo SHALL define daemon optionality, locking, cache lifecycle, compact behavior, and failure recovery for local operation. Watch mode SHALL be explicit and per-repository: `synrepo watch` runs in the foreground for the current repo, `synrepo watch --daemon` starts the same service detached for the current repo, and standalone CLI operation remains fully supported without any daemon.

#### Scenario: Start watch explicitly for one repo
- **WHEN** a user runs `synrepo watch` in an initialized repository
- **THEN** synrepo performs a startup reconcile and enters a foreground watch loop for that repo only
- **AND** no other repository is auto-registered or auto-watched

#### Scenario: Start detached watch mode
- **WHEN** a user runs `synrepo watch --daemon`
- **THEN** synrepo re-execs the same binary through an internal entrypoint and starts a detached watch service for that repo
- **AND** the stdio MCP server remains a separate process model and is not treated as the daemon

### Requirement: Define the first watch-triggered reconcile loop
synrepo SHALL define a watch-triggered update loop that coalesces local filesystem churn into bounded refresh work and uses reconcile passes to correct missed or ambiguous watcher events. The watcher SHALL suppress `.synrepo/` self-events and SHALL remain a trigger layer over the deterministic reconcile path rather than a separate mutation path.

#### Scenario: Observe runtime-only writes under `.synrepo/`
- **WHEN** the watch service sees filesystem activity only inside `.synrepo/`
- **THEN** it ignores those events for structural refresh purposes
- **AND** it does not trigger a new reconcile cycle from its own runtime writes

#### Scenario: Start watch with stale runtime state
- **WHEN** a watch service starts for a repo that may already have stale graph or index state
- **THEN** it runs a startup reconcile before attaching steady-state watcher behavior
- **AND** subsequent watch-triggered work continues to flow through the same reconcile path

### Requirement: Define initial single-writer runtime safety
synrepo SHALL define the initial single-writer safety model for standalone CLI and daemon-assisted operation, including lock acquisition, conflict behavior, and recovery expectations. A long-lived watch lease SHALL record repo-level watch ownership, while `writer.lock` SHALL remain operation-scoped and guard actual runtime mutations only.

#### Scenario: Reconcile while watch service is active
- **WHEN** a user runs `synrepo reconcile` while a watch service already owns the repo
- **THEN** the command forwards a `reconcile_now` request to the active watch service
- **AND** the reconcile work still runs under the normal write lock inside the watch-owned process

#### Scenario: Run another mutating command while watch service is active
- **WHEN** a mutating command that is not watch-aware attempts to write `.synrepo/` while a watch service owns the repo
- **THEN** the command fails with a clear message telling the operator to stop watch first
- **AND** it does not race the active watch owner for writes

### Requirement: Expose initial reconcile and runtime diagnostics
synrepo SHALL expose a small operational diagnostics surface sufficient to understand stale state, recent reconcile outcomes, lock conflicts, maintenance needs, and watch-service ownership for the current runtime.

#### Scenario: Inspect active watch ownership
- **WHEN** a user runs `synrepo watch status` or `synrepo status` while watch mode is active
- **THEN** synrepo reports the watch mode, owner PID, recent reconcile outcome, and stale-artifact state
- **AND** the operator does not have to infer system state from background behavior or orphaned files
