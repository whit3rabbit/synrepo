## ADDED Requirements

### Requirement: Report per-client setup outcomes
`synrepo setup` and `synrepo agent-setup` SHALL report a per-client outcome summary for resolved agent targets.

#### Scenario: Multi-client setup completes
- **WHEN** a user runs setup or agent-setup for multiple targets
- **THEN** the output lists each resolved target with an outcome such as written, registered, current, skipped, unsupported, stale, or failed
- **AND** the output includes the relevant project or global config path when a path is known

#### Scenario: Single-client behavior is used
- **WHEN** a user runs an existing positional invocation such as `synrepo setup claude`
- **THEN** the command preserves existing behavior
- **AND** it may add the per-client outcome summary without changing which files are written

### Requirement: Distinguish detection from mutation
Client detection during setup SHALL be observational until the existing setup confirmation or command execution path performs writes.

#### Scenario: Clients are detected
- **WHEN** setup detects one or more supported clients on the host
- **THEN** the output labels them as detected candidates
- **AND** no shim or MCP config is written solely because detection occurred

### Requirement: Report shim freshness without silent overwrite
Setup reporting SHALL distinguish current, missing, and stale generated shims, and SHALL NOT overwrite stale shims unless the existing `--regen` policy allows it.

#### Scenario: Generated shim is stale
- **WHEN** a generated shim differs from the current template and the user did not request regeneration
- **THEN** setup reports the shim as stale and names the regeneration action
- **AND** the existing shim content is not overwritten silently
