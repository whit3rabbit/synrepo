## ADDED Requirements

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
