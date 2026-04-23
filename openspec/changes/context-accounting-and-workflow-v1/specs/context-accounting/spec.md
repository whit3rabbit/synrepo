## ADDED Requirements

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
