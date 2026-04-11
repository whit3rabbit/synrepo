## ADDED Requirements

### Requirement: Trigger structural graph population during bootstrap
synrepo SHALL run the deterministic structural compile during successful bootstrap and refresh flows after the lexical substrate has been rebuilt, so first-run initialization also materializes the current observed-facts graph.

#### Scenario: Complete bootstrap on a repository with supported inputs
- **WHEN** `synrepo init` succeeds in a repository containing supported code or configured concept markdown
- **THEN** bootstrap triggers the structural compile after rebuilding the lexical substrate
- **AND** the resulting runtime state includes a materialized graph store that reflects current repository inputs

### Requirement: Report graph population status in bootstrap output
synrepo SHALL include graph-oriented status in the bootstrap summary when structural graph population runs, including whether the graph was built or refreshed and whether the runtime is ready for graph inspection commands.

#### Scenario: Review bootstrap output after a graph-producing init
- **WHEN** a bootstrap flow completes after running the structural compile
- **THEN** the user receives status text that distinguishes lexical-index work from graph-population work
- **AND** the next-step guidance remains clear about what graph-oriented commands are now usable
