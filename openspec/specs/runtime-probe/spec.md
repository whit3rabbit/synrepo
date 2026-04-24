## Purpose
Define a read-only runtime probe that classifies a repository's `.synrepo/` runtime state and reports agent-integration readiness, so the CLI entrypoint can route to dashboard, setup wizard, or repair wizard without mutating on-disk state.

## Requirements

### Requirement: Define runtime readiness classification
synrepo SHALL define a runtime probe that classifies the repository's `.synrepo/` runtime state into exactly one of three buckets: `uninitialized`, `partial`, or `ready`. The classification SHALL be deterministic for a given on-disk state and SHALL NOT mutate `.synrepo/` content as a side effect.

#### Scenario: Classify a fresh repository
- **WHEN** the probe runs in a directory that has no `.synrepo/` directory and no `config.toml` under it
- **THEN** the probe returns `uninitialized`
- **AND** the probe does not create `.synrepo/` or any child files

#### Scenario: Classify a fully initialized repository
- **WHEN** `.synrepo/` exists with a readable `config.toml`, a graph store whose compatibility evaluation does not require blocking action, and a status snapshot can be produced
- **THEN** the probe returns `ready`

#### Scenario: Classify a partially initialized repository
- **WHEN** `.synrepo/` exists but one or more required runtime components are missing, corrupt, or blocked by compatibility action
- **THEN** the probe returns `partial`
- **AND** the returned classification carries a structured list of the missing or blocked components

### Requirement: Enumerate required runtime components
synrepo SHALL define the set of components that are required for `ready` classification, distinct from components that are optional and only surfaced as supplementary state.

#### Scenario: Required components present
- **WHEN** `.synrepo/config.toml` is readable, the configured store layout exists with a compatible version, and the runtime can render a status snapshot without error
- **THEN** the probe treats required readiness as satisfied

#### Scenario: Required components absent or blocked
- **WHEN** any of `config.toml`, the store layout, or a successful compatibility evaluation is missing or blocked
- **THEN** the probe records each missing or blocked component and classifies the repo as `partial`

### Requirement: Separate agent-integration readiness
synrepo SHALL report agent-integration readiness (presence of a configured agent shim and presence of MCP registration for the chosen agent target) as a supplementary signal, independent from the required runtime classification.

#### Scenario: Ready runtime with no agent integration
- **WHEN** runtime readiness is `ready` and no agent shim file exists at the configured output path for any supported target
- **THEN** the probe reports agent integration as `absent`
- **AND** the overall classification remains `ready`

#### Scenario: Ready runtime with partial agent integration
- **WHEN** runtime readiness is `ready` and a shim exists but MCP registration for the chosen target is missing
- **THEN** the probe reports agent integration as `partial`
- **AND** the overall classification remains `ready`

#### Scenario: Ready runtime with complete agent integration
- **WHEN** runtime readiness is `ready`, a shim exists at the configured output path, and MCP registration for that target is present
- **THEN** the probe reports agent integration as `complete`

### Requirement: Provide a routing decision for the CLI entrypoint
synrepo SHALL map each probe outcome to a single routing decision used by the bare-`synrepo` entrypoint. Partial state MUST route to a repair path and MUST NOT route to a fresh-install path.

#### Scenario: Uninitialized routes to setup
- **WHEN** the probe returns `uninitialized`
- **THEN** the routing decision selects the guided setup experience

#### Scenario: Partial routes to repair
- **WHEN** the probe returns `partial`
- **THEN** the routing decision selects the guided repair experience with the structured list of missing components
- **AND** the routing decision does not delete or reinitialize existing `.synrepo/` state

#### Scenario: Ready routes to dashboard
- **WHEN** the probe returns `ready`
- **THEN** the routing decision selects the operator dashboard experience
- **AND** the agent-integration signal is carried alongside the decision for display

### Requirement: Probe is non-destructive and side-effect free
synrepo's runtime probe SHALL be read-only. It MUST NOT acquire the writer lock, MUST NOT mutate store contents, and MUST NOT emit log entries into `.synrepo/state/*.jsonl` as part of classification.

#### Scenario: Probe a locked repository
- **WHEN** the probe runs while `.synrepo/state/writer.lock` is held by another process
- **THEN** the probe completes without waiting on or breaking the lock
- **AND** the classification reflects the observable on-disk state

### Requirement: Observational agent-target detection
synrepo SHALL detect likely agent targets by reading file-system hints in the repository and user home directory (for example `.claude/`, `CLAUDE.md`, `.cursor/`, `.codex/`, `.github/copilot-*`, `.windsurf/`). The detection SHALL be purely observational: it MUST NOT write to `.synrepo/`, acquire any lock, or modify the inspected files. The detection output SHALL be a deterministic ordered list of candidate targets for the setup wizard to pre-select from.

#### Scenario: Detect a single agent target
- **WHEN** the probe runs in a repository that contains exactly one supported agent's config hint (for example `.cursor/`)
- **THEN** the agent-target detection returns that target as the first candidate
- **AND** no files on disk are modified

#### Scenario: Detect multiple agent targets
- **WHEN** the probe runs in a repository that contains hints for more than one supported agent
- **THEN** the detection returns all matching targets in a deterministic order
- **AND** the setup wizard uses the first candidate as the default selection while allowing the user to choose another

#### Scenario: No agent target detectable
- **WHEN** the probe runs in a repository with no recognizable agent hints
- **THEN** the detection returns an empty list
- **AND** the setup wizard presents the full target list with no pre-selection and "Skip" as a first-class choice

### Requirement: Provide a capability readiness matrix
The runtime probe SHALL expose a structured capability readiness matrix for core and optional synrepo subsystems.

#### Scenario: Probe reports mixed readiness
- **WHEN** parser coverage is partial, git is unavailable, embeddings are disabled, watch is stopped, overlay is available, and stores are compatible
- **THEN** the probe output includes one readiness row per capability with state, severity, source subsystem, and recommended next action
- **AND** optional disabled capabilities are distinguishable from broken or blocked capabilities

#### Scenario: Compatibility blocks operation
- **WHEN** a store compatibility evaluation blocks graph-backed operation
- **THEN** the readiness matrix marks the affected capability as blocked
- **AND** the recommended next action names `synrepo upgrade` or the existing compatibility recovery path
