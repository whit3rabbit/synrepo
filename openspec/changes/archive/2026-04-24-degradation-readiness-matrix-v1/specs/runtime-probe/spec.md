## ADDED Requirements

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
