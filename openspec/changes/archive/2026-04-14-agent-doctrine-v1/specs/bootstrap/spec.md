## ADDED Requirements

### Requirement: First-run report points to the agent doctrine
synrepo SHALL include a single-line pointer to the agent doctrine in the first-run bootstrap success output. The pointer SHALL name the escalation default (tiny → normal → deep) and reference the shim path most recently written by `synrepo agent-setup`, or a generic pointer (for example the `skill/SKILL.md` path or the `agent-setup` command) when no shim has been generated. The full doctrine block SHALL NOT appear in bootstrap output; only the pointer.

#### Scenario: Clean bootstrap with prior agent-setup
- **WHEN** a user runs `synrepo init` on a repository where `synrepo agent-setup <tool>` has already written a shim
- **AND** bootstrap succeeds with clean health
- **THEN** the success output contains the pointer line naming the escalation default and the shim path
- **AND** the full doctrine block does not appear in the report

#### Scenario: Clean bootstrap without prior agent-setup
- **WHEN** a user runs `synrepo init` on a repository with no prior shim
- **AND** bootstrap succeeds
- **THEN** the success output contains a pointer line naming the escalation default and suggesting the user run `synrepo agent-setup <tool>` or read `skill/SKILL.md`
- **AND** the full doctrine block does not appear

#### Scenario: Partial or failed bootstrap
- **WHEN** bootstrap does not reach clean-success health
- **THEN** the pointer line is not included
- **AND** the output focuses on the health issue rather than agent onboarding
