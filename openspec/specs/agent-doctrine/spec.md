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
The canonical doctrine block SHALL contain (a) the default escalation path (search or entry-point discovery, then `tiny`, then `normal`, then `deep` only before edits or when exact source/body details matter); (b) the overlay advisory rule (commentary and proposed links are labeled machine-authored and freshness-sensitive; `require_freshness=true` only when it matters); (c) the four do-not rules (no large file reads first, no treating commentary as canonical, no triggering synthesis without cause, no expecting background behavior unless watch is explicit); (d) the product-boundary rules (code memory not task memory, handoffs are derived recommendations not canonical planning state, external task systems own assignment and status).

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
