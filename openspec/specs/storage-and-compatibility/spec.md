## Purpose
Define the durable contract for `.synrepo/` storage layout, compatibility-sensitive configuration, migrations, rebuild behavior, and retention boundaries.

## Requirements

### Requirement: Define `.synrepo/` store responsibilities
synrepo SHALL define the purpose of the major `.synrepo/` stores and distinguish compatibility-sensitive state from disposable caches and regenerable artifacts.

#### Scenario: Inspect on-disk state
- **WHEN** a contributor needs to understand what lives under `.synrepo/`
- **THEN** the contract identifies which stores are canonical, supplemental, cached, or ephemeral
- **AND** it distinguishes what may be deleted and rebuilt from what must be migrated or preserved

### Requirement: Define migration and rebuild policy
synrepo SHALL define when schema or format changes require in-place migration, full rebuild, cache invalidation, or explicit user action.

#### Scenario: Upgrade synrepo across a storage change
- **WHEN** a new synrepo version changes a persisted store format or compatibility-sensitive config field
- **THEN** the contract identifies whether the change migrates, rebuilds, invalidates, or refuses to proceed
- **AND** the outcome is deterministic instead of being left to implementation guesswork

### Requirement: Define compatibility-sensitive configuration
synrepo SHALL define which configuration fields affect on-disk compatibility, indexing semantics, graph semantics, or synthesis behavior strongly enough to require rebuild or migration decisions.

#### Scenario: Change a compatibility-sensitive setting
- **WHEN** a user modifies a config field that changes indexing or storage semantics
- **THEN** synrepo can determine whether to rebuild, migrate, warn, or continue
- **AND** the decision is governed by the compatibility contract rather than hidden implementation detail
