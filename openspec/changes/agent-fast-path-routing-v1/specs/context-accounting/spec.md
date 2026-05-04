## ADDED Requirements

### Requirement: Track fast-path routing metrics without content
Context accounting SHALL track fast-path routing usage through aggregate counters only. Persisted metrics SHALL NOT store task descriptions, prompts, paths, source snippets, note bodies, caller identity, or response bodies.

#### Scenario: Task route is classified
- **WHEN** a task route is classified through MCP, CLI, or hook output
- **THEN** context metrics increment `route_classifications_total`
- **AND** the stored metrics contain no task text

#### Scenario: Hook emits fast-path signals
- **WHEN** an agent hook emits context fast-path, deterministic edit candidate, or LLM-not-required signals
- **THEN** context metrics increment the corresponding aggregate hook counters when a repo bucket is available

#### Scenario: Anchored edits complete or reject
- **WHEN** `synrepo_apply_anchor_edits` reports applied or rejected file outcomes
- **THEN** context metrics increment accepted and rejected anchored edit counters
- **AND** no edit text or file path is stored in metrics

#### Scenario: Existing metrics remain readable
- **WHEN** synrepo loads a context metrics file written before fast-path counters existed
- **THEN** missing fast-path fields default to zero
