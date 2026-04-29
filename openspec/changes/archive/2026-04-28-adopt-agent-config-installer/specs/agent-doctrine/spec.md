## ADDED Requirements

### Requirement: Agent shim and skill files are placed via the agent-config installer
synrepo SHALL delegate the placement of agent integration shim and skill files to the `agent-config` installer's `SkillSpec` (for harnesses that support the Agent Skills standard) or `InstructionSpec` (for instruction-only harnesses). The doctrine block content SHALL remain a synrepo-owned compile-time constant (`doctrine_block!()` and the per-target shim modules) and SHALL be supplied to the installer as the spec `body`. The installer SHALL choose the on-disk path; synrepo SHALL NOT maintain a parallel per-harness output-path table for files the installer can place.

#### Scenario: Skill file placement for a SKILL.md harness
- **WHEN** synrepo runs `agent-setup` for a harness that supports skills
- **THEN** the installer's `install_skill` writes the SKILL.md and any sibling assets at the harness-defined location
- **AND** the doctrine content matches the synrepo `doctrine_block!` constant byte-for-byte

#### Scenario: Instruction file placement for an instruction-only harness
- **WHEN** synrepo runs `agent-setup` for an instruction-only harness
- **THEN** the installer's `install_instruction` writes the file using the harness's preferred placement (`ReferencedFile`, `InlineBlock`, or `StandaloneFile`)
- **AND** the synrepo doctrine content is preserved verbatim in the body

### Requirement: Doctrine content remains the source of truth across delegated writes
The compile-time enforcement that every agent-facing surface embeds the canonical doctrine block via `concat!` or equivalent SHALL continue to apply when files are written through the installer. The byte-identical test for shim content SHALL run against the spec bodies passed to the installer, not against post-write file contents alone, so the test fails before any installer call occurs whenever the doctrine content drifts between surfaces.

#### Scenario: Drifted doctrine fails the test before install
- **WHEN** a contributor edits one shim's body without updating the canonical doctrine
- **THEN** the byte-identical test fails on the spec body comparison
- **AND** no installer call is made during the test

### Requirement: Migrate pre-existing shim files into the ownership ledger
For shim and skill files that synrepo already wrote in a previous version (no `_agent_config_tag` marker, no ownership ledger entry), `synrepo upgrade --apply` SHALL adopt them through the installer with `owner = "synrepo"` so subsequent re-runs and removals stay aligned with the installer's ledger.

#### Scenario: Legacy shim adopted on upgrade
- **WHEN** the user runs `synrepo upgrade --apply` against a repository whose `.claude/skills/synrepo/SKILL.md` was written before this change
- **THEN** the upgrade replays the placement through `install_skill`
- **AND** subsequent `synrepo agent-setup --regen` and `synrepo remove` operate via the installer's ledger
