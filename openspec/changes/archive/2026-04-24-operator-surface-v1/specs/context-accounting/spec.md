## MODIFIED Requirements

### Requirement: Track context metrics outside graph truth
synrepo SHALL persist context usage metrics under `.synrepo/state/` and SHALL NOT store them in the graph or overlay stores. Metrics SHALL be inspectable as structured text (default), JSON, and Prometheus exposition format.

#### Scenario: Metrics are inspected
- **WHEN** an operator runs a context metrics command
- **THEN** synrepo returns card counts, token estimate totals, raw-file comparison totals, budget tier usage, escalation counts, latency summaries, stale counts, changed-file counts, and test-surface hits
- **AND** the synthesis pipeline cannot read these metrics as source facts

#### Scenario: Metrics are exported as Prometheus text
- **WHEN** an operator runs `synrepo stats context --format prometheus`
- **THEN** synrepo emits Prometheus exposition text with counters `synrepo_cards_served_total`, `synrepo_card_tokens_total`, `synrepo_raw_file_tokens_total`, `synrepo_estimated_tokens_saved_total`, and `synrepo_stale_responses_total`
- **AND** the output is scrapeable by standard Prometheus tooling without post-processing
