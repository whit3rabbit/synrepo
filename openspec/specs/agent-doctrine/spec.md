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

