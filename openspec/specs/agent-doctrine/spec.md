## Purpose

Define the canonical agent-doctrine block, its distribution across agent-facing surfaces, and the enforcement mechanisms that prevent wording drift.
## Requirements
### Requirement: Maintain a single canonical agent-doctrine block
synrepo SHALL maintain a single canonical agent-doctrine block as a compile-time constant in the repository. Every agent-facing surface (agent-setup shim, `skill/SKILL.md`, card-returning MCP tool descriptions, first-run bootstrap success output) SHALL include the block verbatim or reference it through a compile-time mechanism (`concat!`, `include_str!`, or a shared constant).

#### Scenario: Adding a new agent-facing surface
- **WHEN** a contributor adds a new agent target (for example a future `zed` or `aider` shim)
- **THEN** the new shim embeds the doctrine block through the same `concat!` mechanism as existing shims
- **AND** the byte-identical test in the shims module covers the new shim automatically

#### Scenario: Editing the doctrine
- **WHEN** a contributor edits the canonical doctrine block
- **THEN** every shim that embeds it picks up the change at compile time
- **AND** the `skill/SKILL.md` integration test fails until the corresponding prose is updated to match

### Requirement: Cover escalation, do-not rules, and product boundary
The canonical doctrine block SHALL contain (a) the default escalation path (search or entry-point discovery, then `tiny`, then `normal`, then `deep` only before edits or when exact source/body details matter); (b) the overlay advisory rule (commentary and proposed links are labeled machine-authored and freshness-sensitive; `require_freshness=true` only when it matters); (c) the four do-not rules (no large file reads first, no treating commentary as canonical, no triggering explain without cause, no expecting background behavior unless watch is explicit); (d) the product-boundary rules (code memory not task memory, handoffs are derived recommendations not canonical planning state, external task systems own assignment and status).

#### Scenario: Agent reads any shim
- **WHEN** an agent reads any generated shim (claude, cursor, copilot, generic, codex, windsurf)
- **THEN** the shim contains the default escalation path, the overlay rule, the four do-not rules, and the product-boundary rules
- **AND** the wording of those sections is byte-identical to every other shim

#### Scenario: Agent reads SKILL.md
- **WHEN** an agent loads `skill/SKILL.md`
- **THEN** the file contains the same default escalation path, the same four do-not rules, and the same three product-boundary rules as the shims
- **AND** SKILL.md may expand with additional examples or tool reference material

### Requirement: Enforce byte-identical doctrine across shims
synrepo SHALL enforce byte-identical inclusion of the canonical doctrine block across every shim constant through a compiled test that reads each shim and asserts it contains the block text. The test SHALL fail if a shim is added or edited in a way that diverges from the block.

#### Scenario: Shim drift
- **WHEN** a contributor edits a single shim constant and forgets to update the shared block
- **THEN** the byte-identical test fails with a message identifying the diverging shim
- **AND** the change cannot be merged until the shim re-embeds the canonical block

#### Scenario: Block edit without shim update
- **WHEN** a contributor edits the canonical block but does not recompile
- **THEN** the next `cargo build` picks up the change for every shim via `concat!`
- **AND** no manual copy-paste across shims is required

### Requirement: Teach the context workflow
Agent-facing doctrine SHALL teach the workflow as orient, find cards, inspect impact (via `synrepo_impact` or its shorthand `synrepo_risks`), edit, validate tests, and check changed context.

#### Scenario: Agent reads generated instructions
- **WHEN** an agent reads the generated synrepo doctrine or skill file
- **THEN** the instructions tell the agent to start with synrepo context before large cold file reads
- **AND** the instructions identify the workflow aliases (including `synrepo_risks` as a shorthand for `synrepo_impact`) and the budget escalation rule

### Requirement: Require bounded-context workflow guidance
The canonical agent doctrine SHALL state the preferred workflow: orient first, find bounded cards, inspect impact or risks before edits, validate tests, and check changed context before claiming completion.

#### Scenario: Generated shim includes workflow guidance
- **WHEN** synrepo generates or regenerates an agent shim
- **THEN** the shim includes the bounded-context workflow guidance
- **AND** the guidance tells agents to use full-file reads only after cards identify the relevant target or when bounded cards are insufficient

#### Scenario: Doctrine remains source-truth safe
- **WHEN** the workflow guidance mentions overlay notes, commentary, or advisory content
- **THEN** it states that graph-backed structural facts remain authoritative
- **AND** it does not imply overlay content can define source truth

### Requirement: Use canonical doctrine for shim freshness checks
Generated shim freshness SHALL be evaluated against the canonical agent doctrine and current target-specific template content.

#### Scenario: Doctrine block changes
- **WHEN** the canonical doctrine block changes after a shim was generated
- **THEN** setup or agent-setup can classify the existing shim as stale
- **AND** the report points to `--regen` or the existing regeneration flow rather than embedding divergent doctrine text

#### Scenario: Shim is current
- **WHEN** an existing generated shim matches the current canonical doctrine and target template
- **THEN** setup reports the shim as current
- **AND** no write is performed for that shim

### Requirement: Guide agents to pass repo_root for global MCP use
Generated agent guidance SHALL explain that a global synrepo MCP integration serves registered projects by absolute repository path. In global or defaultless contexts, agents SHALL pass the current workspace's absolute path as `repo_root` to repo-addressable tools.

#### Scenario: Agent reads global integration guidance
- **WHEN** an agent reads generated synrepo doctrine or a generated shim after global integration support exists
- **THEN** the guidance tells the agent to pass the current workspace root as `repo_root` when using a global MCP server
- **AND** the guidance preserves the existing orient, find, impact, edit, tests, changed workflow

#### Scenario: Repository is not registered
- **WHEN** a global MCP tool reports that a repository is not managed by synrepo
- **THEN** the guidance tells the agent to ask the user to run `synrepo project add <path>`
- **AND** the guidance does not imply the agent should bypass registry gating

### Requirement: Preserve repo-bound default behavior in doctrine
Generated agent guidance SHALL state that repo-bound MCP configurations may omit `repo_root` because the server has a default repository, but passing the absolute repository root remains valid and preferred when an agent can identify it reliably.

#### Scenario: Agent uses project-scoped MCP config
- **WHEN** an agent is operating through a project-scoped MCP config that launches `synrepo mcp --repo .`
- **THEN** the guidance permits omitting `repo_root`
- **AND** it does not contradict the global guidance to pass `repo_root` when using a global server

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

