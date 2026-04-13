## ADDED Requirements

### Requirement: Define the upgrade command as the explicit migration entry point
synrepo SHALL treat `synrepo upgrade` as the single explicit entry point for applying compatibility actions to `.synrepo/` stores after a binary version change. The upgrade command SHALL reuse the compatibility evaluator to determine the required action per store and SHALL require `--apply` before executing any mutating actions.

#### Scenario: Inspect stores after a binary update
- **WHEN** a user runs `synrepo upgrade` without `--apply`
- **THEN** the command reads each store's recorded schema version, applies the compatibility evaluator, and prints a plan showing each store, its version, and the required action
- **AND** no stores are mutated
- **AND** the command exits with a non-zero code if any store requires action (to support scripted upgrade pipelines)

#### Scenario: Apply compatibility actions
- **WHEN** a user runs `synrepo upgrade --apply`
- **THEN** each store receives its required compatibility action (continue / rebuild / invalidate / clear-and-recreate / migrate)
- **AND** `block` actions prevent the upgrade from continuing and emit a clear error with manual steps
- **AND** the command exits zero if all stores reach a usable state

#### Scenario: Detect version skew at startup without upgrade
- **WHEN** synrepo starts and detects that a store's version is outside the supported range without the user running `upgrade`
- **THEN** the startup path applies the compatibility action defined for that store (typically `block` for unsupported-newer versions)
- **AND** the error message includes the suggested `synrepo upgrade` invocation
- **AND** canonical stores are never silently deleted on startup to resolve version skew
