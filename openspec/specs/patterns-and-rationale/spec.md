## Purpose
Define the optional human-guidance layer for patterns, ADRs, inline rationale, and DecisionCards without making prose mandatory for value.

## Requirements

### Requirement: Define optional human-guidance inputs
synrepo SHALL define optional pattern documents, ADRs, and inline `# DECISION:` markers as a human-guidance layer. Pattern files are markdown documents in configured concept directories (same directories as ConceptNodes: `docs/concepts/`, `docs/adr/`, `docs/decisions/` by default). Inline `# DECISION:` markers are line comments in code files using the language-appropriate comment prefix followed by the exact token `DECISION:` and non-empty text. Both sources are optional; their absence does not affect structural card delivery.

#### Scenario: Use synrepo on a code-only repository
- **WHEN** a repository has no pattern documents, ADRs, or `# DECISION:` markers
- **THEN** structural cards are returned unchanged
- **AND** no DecisionCard fields appear in card output
- **AND** no errors or warnings are emitted about missing rationale

#### Scenario: Detect an inline decision marker
- **WHEN** a code file contains a line comment matching `// DECISION:` (or language-equivalent prefix) followed by non-empty decision text
- **THEN** the structural compile extracts the decision text and the containing file path
- **AND** the extracted marker is stored as an `inline_decisions` entry on the containing `FileNode`

### Requirement: Define rationale extraction rules for ADRs and pattern files
synrepo SHALL extract structured rationale from human-authored markdown in concept directories. The extractor SHALL parse frontmatter fields `title`, `status`, and `governs` (array of relative file paths). The decision body SHALL be extracted as the markdown content following the first `## Decision` heading, or the full body if no such heading exists. Fields not present in the frontmatter are treated as absent, not as an error.

#### Scenario: Extract rationale from an ADR with frontmatter
- **WHEN** a markdown file in a concept directory contains YAML frontmatter with `title`, `status`, and `governs` fields
- **THEN** the extractor produces a ConceptNode with the title as display name
- **AND** the status and decision body are stored as ConceptNode metadata
- **AND** a `Governs` edge is created for each file path listed in the `governs` array that resolves to a known FileNodeId

#### Scenario: Extract rationale from an ADR without governs frontmatter
- **WHEN** a markdown file in a concept directory has no `governs` field
- **THEN** a ConceptNode is produced as usual
- **AND** no Governs edges are emitted from path references
- **AND** the file is still fully indexed

#### Scenario: Handle a stale governs reference
- **WHEN** a `governs` array entry references a file path that does not exist in the current graph
- **THEN** no Governs edge is emitted for that entry
- **AND** no error halts the compile
- **AND** the ConceptNode is still produced

### Requirement: Define Governs edge emission rules
synrepo SHALL emit `EdgeKind::Governs` only from human-authored ADR or pattern frontmatter `governs` arrays. Governs edges SHALL be labeled `HumanDeclared`. Inline `# DECISION:` markers are stored on the containing `FileNode` and do not emit `Governs` edges in this change. The explain pipeline and overlay store SHALL NOT emit Governs edges.

#### Scenario: Governs edge from frontmatter
- **WHEN** an ADR frontmatter contains `governs: [src/store/sqlite/mod.rs]`
- **THEN** the compile emits a `Governs` edge from the ConceptNode to the FileNode for `src/store/sqlite/mod.rs`
- **AND** the edge is labeled `HumanDeclared`

#### Scenario: No machine-authored Governs edges
- **WHEN** the overlay contains machine-authored content referencing a code node
- **THEN** no `Governs` edge is emitted from that reference
- **AND** the Governs edge count in the graph is unchanged

### Requirement: Define promotion rules for rationale material
synrepo SHALL define when human-authored rationale sources receive `HumanDeclared` epistemic label. ConceptNodes from configured concept directories are promoted to `HumanDeclared` when the repository is in curated mode. In auto mode, ConceptNodes remain `ParserObserved`. Governs edges from human-authored sources are labeled `HumanDeclared` in both modes. Machine-authored overlay content is never promoted to `HumanDeclared`.

#### Scenario: Promote a rationale source in curated mode
- **WHEN** a repository is in curated mode and a markdown file exists in a configured concept directory
- **THEN** the ConceptNode produced from that file is labeled `HumanDeclared`
- **AND** any Governs edges from its frontmatter are also labeled `HumanDeclared`

#### Scenario: Governs edges are always HumanDeclared
- **WHEN** a `# DECISION:` marker is found in a code file in auto mode
- **THEN** the inline decision text is stored on the containing `FileNode`
- **AND** the auto/curated mode setting does not affect inline decision storage

### Requirement: Define DecisionCard behavior
synrepo SHALL define DecisionCards as optional outputs backed by human-authored rationale sources and linked to structural cards without overriding descriptive truth.

#### Scenario: Ask why a subsystem exists
- **WHEN** an agent requests rationale for a target with linked human-authored decision material
- **THEN** the system can return a DecisionCard
- **AND** the response distinguishes rationale from current code behavior
