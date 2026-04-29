## ADDED Requirements

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
