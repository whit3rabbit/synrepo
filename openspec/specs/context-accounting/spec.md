## Purpose

Define the context-accounting contract: the shared metadata object attached to card-shaped responses, the operational metrics store for context usage, and the benchmark surface that validates context-savings claims. Accounting is observational — it is derived from source, graph, or response shape, never from LLM output, and never stored in the graph or overlay stores.
## Requirements
### Requirement: Report context accounting on card responses
synrepo SHALL attach a shared `context_accounting` object to every card-shaped CLI or MCP response.

#### Scenario: Agent receives a card
- **WHEN** an agent requests a card-shaped response
- **THEN** the response includes `context_accounting.budget_tier`, `token_estimate`, `raw_file_token_estimate`, `estimated_savings_ratio`, `source_hashes`, `stale`, and `truncation_applied`
- **AND** the accounting metadata is derived from source, graph, or response shape rather than LLM output

### Requirement: Track context metrics outside graph truth
synrepo SHALL persist context usage metrics under `.synrepo/state/` and SHALL NOT store them in the graph or overlay stores.

#### Scenario: Metrics are inspected
- **WHEN** an operator runs a context metrics command
- **THEN** synrepo returns card counts, token estimate totals, raw-file comparison totals, budget tier usage, escalation counts, latency summaries, stale counts, changed-file counts, and test-surface hits
- **AND** the synthesis pipeline cannot read these metrics as source facts

### Requirement: Benchmark context savings and usefulness
synrepo SHALL provide a reproducible benchmark for context tasks that reports both compression and whether expected files, symbols, or tests were included.

#### Scenario: Context benchmark runs
- **WHEN** an operator runs `synrepo bench context --tasks <glob> --json`
- **THEN** the report includes raw file tokens, card tokens, reduction ratio, target hit or miss, stale rate, latency, and test-link coverage
- **AND** no benchmark claim is reduced to token savings alone

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

