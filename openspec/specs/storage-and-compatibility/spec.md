## Purpose
Define the durable contract for `.synrepo/` storage layout, compatibility-sensitive configuration, migrations, rebuild behavior, and retention boundaries.

## Requirements

### Requirement: Define `.synrepo/` store responsibilities
synrepo SHALL define the purpose, durability class, and compatibility owner of the major `.synrepo/` stores and distinguish compatibility-sensitive state from disposable caches and regenerable artifacts.

#### Scenario: Inspect on-disk state
- **WHEN** a contributor needs to understand what lives under `.synrepo/`
- **THEN** the contract identifies which stores are canonical, supplemental, rebuildable, disposable, or ephemeral
- **AND** it distinguishes what may be deleted and rebuilt from what must be migrated or preserved
- **AND** it defines the default compatibility action for each current store, including `graph`, `overlay`, `index`, `embeddings`, `cache/llm-responses`, and `state`

### Requirement: Define migration and rebuild policy
synrepo SHALL define when schema or format changes require in-place migration, full rebuild, cache invalidation, clear-and-recreate, safe continue, or explicit user action.

#### Scenario: Upgrade synrepo across a storage change
- **WHEN** a new synrepo version changes a persisted store format or compatibility-sensitive config field
- **THEN** the contract identifies whether the change continues safely, rebuilds, invalidates, clears and recreates, migrates, or refuses to proceed
- **AND** the outcome is deterministic instead of being left to implementation guesswork
- **AND** canonical stores are never silently deleted to satisfy compatibility

### Requirement: Define compatibility-sensitive configuration
synrepo SHALL define which configuration fields affect on-disk compatibility, indexing semantics, graph semantics, or synthesis behavior strongly enough to require rebuild, invalidation, warning, or migration decisions.

#### Scenario: Change a compatibility-sensitive setting
- **WHEN** a user modifies a config field that changes indexing or storage semantics
- **THEN** synrepo can determine whether to rebuild, invalidate, migrate, warn, or continue
- **AND** the decision is governed by the compatibility contract rather than hidden implementation detail

#### Scenario: Group current config fields by compatibility impact
- **WHEN** a contributor inspects the compatibility-sensitive config contract
- **THEN** `roots`, `max_file_size_bytes`, and `redact_globs` are classified as discovery and index compatibility inputs
- **AND** `concept_directories` and `git_commit_depth` are classified as future graph or overlay compatibility inputs that may require guidance before those stores are fully implemented
- **AND** `mode` is treated as operational configuration, not a persisted-store compatibility trigger
