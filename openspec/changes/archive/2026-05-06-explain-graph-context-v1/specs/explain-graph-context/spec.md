## ADDED Requirements

### Requirement: Build commentary prompts from graph-backed context
Commentary generation SHALL use a shared prompt context builder backed by canonical graph facts and source snippets. The builder SHALL support both file and symbol commentary targets and SHALL include direct graph neighborhood context when available.

#### Scenario: Generate symbol commentary with graph neighborhood
- **WHEN** commentary is generated for a symbol with direct call or import neighbors
- **THEN** the prompt includes the target symbol facts and bounded associated node context from canonical graph edges

#### Scenario: Generate file commentary with graph neighborhood
- **WHEN** commentary is generated for a file with direct import relationships
- **THEN** the prompt includes the target file facts and bounded import/imported-by context from canonical graph edges

### Requirement: Enforce commentary input budget before provider calls
The shared context builder SHALL respect the configured commentary input token limit by trimming lower-priority context blocks before provider invocation. Provider-side budget blocking SHALL remain a final guard.

#### Scenario: Trim optional context under small budget
- **WHEN** the configured commentary input token limit cannot fit every optional context block
- **THEN** the builder omits lower-priority associated context before omitting target facts
- **AND** the generated prompt remains structured enough to identify the target

### Requirement: Preserve explain trust boundaries
The shared context builder MUST NOT use overlay commentary, proposed links, materialized explain docs, or any other machine-authored overlay output as explain input.

#### Scenario: Overlay output is not prompt input
- **WHEN** commentary exists in the overlay for the target or its neighbors
- **THEN** the generated prompt is built only from graph, source, git-observed, and human-declared decision facts
- **AND** no overlay commentary text or proposed-link content appears in the prompt

### Requirement: Keep graph context bounded to degree one
Explain graph context SHALL include only the target node and directly connected graph neighbors in v1. Transitive graph expansion beyond one degree SHALL be out of scope unless a future change updates this contract.

#### Scenario: Degree-one prompt context
- **WHEN** commentary is generated for a target that has neighbors with their own neighbors
- **THEN** the prompt includes the target's direct neighbors only
- **AND** it does not recursively include second-degree graph neighbors
