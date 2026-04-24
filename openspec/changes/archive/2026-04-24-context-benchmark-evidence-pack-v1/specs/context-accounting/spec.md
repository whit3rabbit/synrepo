## ADDED Requirements

### Requirement: Provide fixture-backed context benchmark reports
`synrepo bench context` SHALL support checked-in task fixtures that produce stable benchmark reports containing compression, usefulness, freshness, and latency fields.

#### Scenario: Run checked-in context benchmark fixtures
- **WHEN** an operator runs `synrepo bench context --tasks "benches/tasks/*.json" --json`
- **THEN** the report includes one entry per valid fixture with task name, category, query, baseline kind, raw file tokens, card tokens, reduction ratio, target hits, target misses, stale rate, latency, and returned targets
- **AND** the JSON field names are stable across patch releases unless a documented benchmark schema version changes

#### Scenario: Required context is absent
- **WHEN** a fixture names required files, symbols, or tests that are not returned by the benchmarked card path
- **THEN** the task report marks those targets as misses
- **AND** token reduction is still reported but is not treated as a successful context-saving result

### Requirement: Keep benchmark accounting outside graph truth
Context benchmark results SHALL remain operational evidence and SHALL NOT write benchmark outcomes into graph or overlay truth stores.

#### Scenario: Benchmark completes
- **WHEN** a context benchmark run finishes
- **THEN** any persisted data is limited to operational metrics or explicit benchmark output
- **AND** no graph node, graph edge, overlay commentary, overlay note, or proposed link is created from benchmark results
