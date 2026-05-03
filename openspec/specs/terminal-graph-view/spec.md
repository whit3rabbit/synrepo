## Purpose
Define the terminal graph runtime view as a bounded, graph-backed convenience surface that shares graph-neighborhood model behavior without becoming canonical graph truth, overlay truth, or explain input.

## Requirements

### Requirement: Terminal graph view is bounded and graph-backed
synrepo SHALL provide a terminal graph runtime view that renders a bounded neighborhood from canonical graph nodes and active edges. The runtime view SHALL be a convenience surface only and SHALL NOT become graph truth, overlay truth, or explain input.

#### Scenario: User opens a bounded terminal graph view
- **WHEN** a user runs `synrepo graph view <target>` in a TTY
- **THEN** synrepo resolves `<target>` as a node ID, file path, qualified symbol name, or short symbol name
- **AND** synrepo opens an interactive terminal view of the bounded graph neighborhood

#### Scenario: Terminal graph view has no target
- **WHEN** a user runs `synrepo graph view` without a target
- **THEN** synrepo renders a deterministic top-degree overview bounded by the configured limit

#### Scenario: Terminal graph view is non-canonical
- **WHEN** synrepo renders the terminal graph view
- **THEN** the output is labeled or documented as a runtime convenience view
- **AND** no explain pipeline reads the view as graph truth or provider input

### Requirement: Graph view model is shared and bounded
synrepo SHALL expose a shared graph-neighborhood model for graph view consumers. The model SHALL include target, focal node ID, direction, depth, edge type filters, counts, truncation state, compact nodes, and compact edges with provenance and epistemic labels.

#### Scenario: JSON graph view is requested
- **WHEN** a user runs `synrepo graph view <target> --json`
- **THEN** synrepo prints the shared graph-neighborhood model as JSON
- **AND** the command works without a TTY

#### Scenario: Traversal exceeds limits
- **WHEN** the requested neighborhood has more records than the configured limit permits
- **THEN** synrepo returns a bounded response
- **AND** the response marks `truncated` as true

### Requirement: Graph view supports terminal navigation
synrepo SHALL support basic keyboard navigation in the terminal graph view: selected-node movement, refocusing, depth adjustment, direction selection, filtering, and quit.

#### Scenario: User navigates graph nodes
- **WHEN** the terminal graph view is open
- **THEN** arrow keys move the selected node, Enter refocuses on it, plus and minus adjust depth, `i`/`o`/`b` select direction, `/` filters labels, and Esc or `q` exits
