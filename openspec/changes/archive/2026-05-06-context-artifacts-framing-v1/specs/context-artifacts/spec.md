## ADDED Requirements

### Requirement: Define context artifact layers
synrepo SHALL define its context product model as `repo files -> graph facts -> code artifacts -> task contexts -> cards/MCP`. Graph facts SHALL be canonical observed facts. Code artifacts SHALL be deterministic compiled records derived from graph, substrate, git, and approved human-authored inputs. Task contexts SHALL be bounded bundles of artifacts for a workflow. Cards and MCP responses SHALL be delivery packets for those artifacts and contexts.

#### Scenario: Contributor describes the product model
- **WHEN** a contributor updates product docs, agent guidance, or OpenSpec text
- **THEN** the text can distinguish graph facts, code artifacts, task contexts, and cards/MCP delivery
- **AND** it does not collapse overlay output into canonical graph facts

### Requirement: Keep framing separate from runtime storage
The context-artifact framing SHALL NOT imply a persistent artifact registry, new storage table, new cache, or new invalidation workflow unless a later change explicitly specifies one.

#### Scenario: Framing-only change is implemented
- **WHEN** this change is applied
- **THEN** no runtime storage path, SQLite schema, JSONL artifact cache, or migration is added
- **AND** existing cards and context packs remain the delivery mechanisms

### Requirement: Preserve graph and overlay trust boundaries
Context artifact language SHALL preserve the existing trust model: graph-backed facts remain authoritative, while overlay commentary, proposed links, explain docs, and agent notes remain advisory and freshness-labeled.

#### Scenario: Artifact text references overlay content
- **WHEN** docs describe a task context that includes advisory overlay content
- **THEN** the docs label that content as overlay-backed and advisory
- **AND** graph-backed facts remain the canonical source for code truth
