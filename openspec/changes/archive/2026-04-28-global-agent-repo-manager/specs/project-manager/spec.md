## ADDED Requirements

### Requirement: Register managed projects
Synrepo SHALL provide a `synrepo project add [path]` command that registers a repository in the user-level project registry. The command SHALL canonicalize the target path, initialize `.synrepo/` when the target is uninitialized, preserve existing initialized state when present, and record the project only after the repository reaches a usable initialized state.

#### Scenario: Add an uninitialized repository
- **WHEN** the user runs `synrepo project add /work/app` and `/work/app` has no `.synrepo/` state
- **THEN** synrepo initializes the repository using the existing bootstrap/init semantics
- **AND** the registry records the canonical absolute path for `/work/app`

#### Scenario: Add an already initialized repository
- **WHEN** the user runs `synrepo project add /work/app` and `/work/app` is already initialized
- **THEN** synrepo leaves the existing repository state intact
- **AND** the registry contains a single project entry for the canonical path

#### Scenario: Initialization cannot reach usable state
- **WHEN** project registration cannot initialize or validate the target repository
- **THEN** synrepo reports the blocking health issue
- **AND** the registry is not updated with a project that cannot be served

### Requirement: List managed project health
Synrepo SHALL provide a `synrepo project list` command that reads the user-level registry and reports each managed project's path and current read-only health. Health SHALL be derived at command time from the repository state and store compatibility, not persisted as registry truth.

#### Scenario: List projects as text
- **WHEN** the user runs `synrepo project list`
- **THEN** synrepo prints every registered project with its canonical path and health classification
- **AND** missing, partial, or incompatible repositories are labeled without mutating them

#### Scenario: List projects as JSON
- **WHEN** the user runs `synrepo project list --json`
- **THEN** synrepo emits a machine-readable list containing each path, registry metadata, and derived health
- **AND** the command remains read-only

### Requirement: Inspect one managed project
Synrepo SHALL provide a `synrepo project inspect [path]` command that reports the registry entry and current health for one project. If the path is not registered, the command SHALL report that it is unmanaged and suggest `synrepo project add <path>`.

#### Scenario: Inspect registered project
- **WHEN** the user runs `synrepo project inspect /work/app` for a registered project
- **THEN** synrepo reports the stored project entry and current derived health

#### Scenario: Inspect unmanaged project
- **WHEN** the user runs `synrepo project inspect /work/app` for a path not present in the registry
- **THEN** synrepo reports that the repository is not managed by synrepo
- **AND** the output names `synrepo project add /work/app` as the registration action

### Requirement: Unregister managed projects without deleting repository state
Synrepo SHALL provide a `synrepo project remove [path]` command that removes the matching project entry from `~/.synrepo/projects.toml`. The command SHALL NOT delete `.synrepo/`, generated shims, or agent MCP configuration files.

#### Scenario: Remove registered project
- **WHEN** the user runs `synrepo project remove /work/app`
- **THEN** synrepo removes the canonical project entry from the registry
- **AND** the repository's `.synrepo/` directory remains on disk

#### Scenario: Remove unmanaged project
- **WHEN** the user runs `synrepo project remove /work/app` and no matching registry entry exists
- **THEN** synrepo reports that no managed project was found
- **AND** no filesystem state is deleted

### Requirement: Preserve registry compatibility
The project registry SHALL remain backward compatible with existing `~/.synrepo/projects.toml` files. New fields SHALL use serde defaults, corrupt TOML SHALL remain a loud error, and project paths SHALL continue to be compared by canonical path.

#### Scenario: Load existing registry
- **WHEN** synrepo loads a registry created before project-manager fields existed
- **THEN** missing fields receive defaults
- **AND** existing project and agent install records remain usable

#### Scenario: Registry TOML is corrupt
- **WHEN** synrepo cannot parse `~/.synrepo/projects.toml`
- **THEN** project commands and global MCP registry checks report the parse error
- **AND** synrepo does not silently treat the corrupt registry as empty
