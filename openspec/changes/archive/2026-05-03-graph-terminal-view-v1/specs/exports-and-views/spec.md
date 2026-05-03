## ADDED Requirements

### Requirement: Define terminal graph view as a runtime view
synrepo SHALL classify `synrepo graph view` as an internal runtime view distinct from explicit exports. The view SHALL be derived from canonical graph state at read time and SHALL NOT write export artifacts unless the user separately runs `synrepo export`.

#### Scenario: User opens terminal graph view
- **WHEN** a user runs `synrepo graph view`
- **THEN** synrepo renders a runtime view from current graph state
- **AND** no `synrepo-context/` export artifact is created by the view command

#### Scenario: Explain runs after terminal graph view
- **WHEN** an explain or retrieval pipeline runs after a terminal graph view was opened
- **THEN** the terminal graph view output is not used as explain input
- **AND** canonical graph and human-authored source remain the only graph/source inputs
