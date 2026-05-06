# project-manager Specification

## Purpose
TBD - created by archiving change global-agent-repo-manager. Update Purpose after archive.
## Requirements
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

### Requirement: Preserve stable project identity
Synrepo SHALL assign every managed project a stable project ID that is independent of folder name and display alias. The registry SHALL remain backward compatible with path-only entries by deriving an effective ID from the canonical path when an explicit ID is missing.

#### Scenario: Legacy project entry loads
- **WHEN** synrepo loads a registry entry that contains a path and initialized timestamp but no project ID
- **THEN** the project remains usable
- **AND** synrepo exposes an effective stable project ID for selection and MCP/TUI routing

#### Scenario: Duplicate folder names remain distinct
- **WHEN** the registry contains `/work/main/synrepo` and `/work/fork/synrepo`
- **THEN** each project has a distinct project ID
- **AND** name-based selection reports ambiguity unless the user provides an ID or unambiguous path

### Requirement: Support project display aliases
Synrepo SHALL support an optional display name for a managed project. If no display name is set, the user-facing name SHALL default to the repository folder name. Renaming a project SHALL update only the registry display alias and SHALL NOT rename folders, move storage, or alter agent install metadata.

#### Scenario: Rename preserves project storage
- **WHEN** the user renames a managed project
- **THEN** the project keeps the same ID, canonical path, `.synrepo/` directory, agents, hooks, and initialized timestamp
- **AND** subsequent project lists show the new display name

### Requirement: Resolve projects by ID, name, or path
Synrepo SHALL provide project lookup helpers and CLI commands that resolve a project by stable ID, display name, or canonical path. Name lookup SHALL fail loudly when multiple projects share the same display name.

#### Scenario: Ambiguous name
- **WHEN** the user selects a project by a name shared by multiple registered projects
- **THEN** synrepo reports the ambiguity
- **AND** the error lists matching project IDs and paths

#### Scenario: Project use records recency
- **WHEN** the user selects a project with `synrepo project use <id-or-name-or-path>`
- **THEN** synrepo resolves the project
- **AND** updates `last_opened_at` for that project
- **AND** reports the selected project ID, display name, and path

### Requirement: Detach remains non-destructive
Synrepo SHALL treat project removal from the project manager as registry detachment only. The operation SHALL NOT delete `.synrepo/`, generated exports, shims, MCP configs, hooks, graph stores, or overlay stores.

#### Scenario: Detach project from TUI or CLI
- **WHEN** the user removes or detaches a managed project
- **THEN** synrepo removes the project entry from `~/.synrepo/projects.toml`
- **AND** repository-local state remains untouched

