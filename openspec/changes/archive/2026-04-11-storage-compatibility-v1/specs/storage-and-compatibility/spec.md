## MODIFIED Requirements

### Requirement: Define `.synrepo/` store responsibilities
synrepo SHALL classify the major `.synrepo/` stores by durability and operational role, distinguishing canonical persisted state from supplemental stores, caches, and regenerable artifacts.

#### Scenario: Inspect the initialized runtime layout
- **WHEN** a contributor or command inspects `.synrepo/` after bootstrap
- **THEN** each major store has a declared durability class and purpose
- **AND** the system can tell which stores may be deleted and rebuilt versus which must be preserved or migrated

### Requirement: Define migration and rebuild policy
synrepo SHALL define rebuild, invalidate, migrate, or refuse-to-proceed behavior separately for each store class when storage formats or compatibility-sensitive settings change.

#### Scenario: Change a persisted store format
- **WHEN** a synrepo upgrade changes the expected format of a persisted runtime store
- **THEN** the storage contract determines whether synrepo migrates the store, rebuilds it, invalidates it, or blocks until explicit maintenance occurs
- **AND** the outcome is the same regardless of which command encounters the incompatibility first

### Requirement: Define compatibility-sensitive configuration
synrepo SHALL identify config fields whose changes affect discovery semantics, index contents, graph semantics, Git-history mining, or persisted-store compatibility strongly enough to require operational action.

#### Scenario: Lower Git history depth in config
- **WHEN** a user changes a compatibility-sensitive config field such as repository roots, max file size, redact globs, or Git history depth
- **THEN** synrepo can determine whether to warn, rebuild, invalidate, migrate, or continue safely
- **AND** the decision follows the declared compatibility policy instead of hidden implementation behavior
