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
synrepo SHALL persist context usage metrics under `.synrepo/state/` and SHALL NOT store them in the graph or overlay stores. Metrics SHALL be inspectable as structured text (default), JSON, and Prometheus exposition format.

#### Scenario: Metrics are inspected
- **WHEN** an operator runs a context metrics command
- **THEN** synrepo returns card counts, token estimate totals, raw-file comparison totals, budget tier usage, escalation counts, latency summaries, stale counts, changed-file counts, and test-surface hits
- **AND** the synthesis pipeline cannot read these metrics as source facts

#### Scenario: Metrics are exported as Prometheus text
- **WHEN** an operator runs `synrepo stats context --format prometheus`
- **THEN** synrepo emits Prometheus exposition text with counters `synrepo_cards_served_total`, `synrepo_card_tokens_total`, `synrepo_raw_file_tokens_total`, `synrepo_estimated_tokens_saved_total`, and `synrepo_stale_responses_total`
- **AND** the output is scrapeable by standard Prometheus tooling without post-processing

### Requirement: Benchmark context savings and usefulness
synrepo SHALL provide a reproducible benchmark for context tasks that reports compression, grounding, and whether expected files, symbols, or tests were included.

#### Scenario: Context benchmark runs
- **WHEN** an operator runs `synrepo bench context --tasks <glob> --mode all --json`
- **THEN** the report includes raw file tokens, card tokens, reduction ratio, target hit or miss, stale rate, latency, task success, tokens returned, citation coverage, span coverage, wrong-context rate when `allowed_context` is present, and test-link coverage
- **AND** no benchmark claim is reduced to token savings alone

### Requirement: Provide fixture-backed context benchmark reports
`synrepo bench context` SHALL support checked-in task fixtures that produce stable benchmark reports containing compression, usefulness, freshness, grounding, wrong-context, and latency fields.

#### Scenario: Run checked-in context benchmark fixtures
- **WHEN** an operator runs `synrepo bench context --tasks "benches/tasks/*.json" --mode all --json`
- **THEN** the report includes one entry per valid fixture with task name, category, query, baseline kind, raw file tokens, card tokens, reduction ratio, target hits, target misses, stale rate, latency, returned targets, and per-strategy runs under `runs`
- **AND** the JSON field names are stable across patch releases unless a documented benchmark schema version changes

#### Scenario: Run historical cards-only context benchmark
- **WHEN** an operator runs `synrepo bench context --tasks "benches/tasks/*.json" --json`
- **THEN** the default mode is `cards`
- **AND** the report keeps the historical task-level card fields as compatibility aliases for the cards run

#### Scenario: Required context is absent
- **WHEN** a fixture names required files, symbols, or tests that are not returned by the benchmarked card path
- **THEN** the task report marks those targets as misses
- **AND** token reduction is still reported but is not treated as a successful context-saving result

#### Scenario: Wrong-context rate is not measurable
- **WHEN** a fixture omits `allowed_context`
- **THEN** every strategy run reports `wrong_context_rate` as null
- **AND** null is not rendered or interpreted as zero

### Requirement: Keep benchmark accounting outside graph truth
Context benchmark results SHALL remain operational evidence and SHALL NOT write benchmark outcomes into graph or overlay truth stores.

#### Scenario: Benchmark completes
- **WHEN** a context benchmark run finishes
- **THEN** any persisted data is limited to operational metrics or explicit benchmark output
- **AND** no graph node, graph edge, overlay commentary, overlay note, or proposed link is created from benchmark results

### Requirement: Expose trust-ready context metric aggregates
Context accounting SHALL provide aggregate fields suitable for dashboard trust rendering without requiring the dashboard to parse individual card responses.

#### Scenario: Dashboard consumes context metrics
- **WHEN** the dashboard requests context trust data from the shared status snapshot
- **THEN** the snapshot exposes cards served, average card tokens, estimated tokens avoided, stale responses, truncation counts, and escalation counts when metrics exist
- **AND** the dashboard does not read `.synrepo/state/context-metrics.json` through a second ad hoc path

#### Scenario: Context metrics are absent
- **WHEN** no context metrics file or counters exist
- **THEN** the snapshot reports the metric group as absent
- **AND** renderers can distinguish absent metrics from zero-value metrics

### Requirement: Track observable workflow usage counters
Context accounting SHALL track observable workflow tool usage separately from estimated context-savings counters.

#### Scenario: Workflow tools are used
- **WHEN** an agent invokes orient, find, explain, impact, risks, tests, changed, or minimum-context through synrepo
- **THEN** context metrics can report per-tool usage counts
- **AND** those counts are labeled as observed synrepo calls

#### Scenario: Cold-read avoidance is estimated
- **WHEN** synrepo reports full-file-read avoidance or estimated raw tokens avoided
- **THEN** the metric is labeled as estimated from card accounting data
- **AND** it is not presented as proof that an external agent did not read files outside synrepo

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

### Requirement: Report compact output accounting
Compact MCP responses SHALL include `output_accounting` with deterministic token estimates for returned output and the original uncompact response, an estimated token savings value, an estimated savings ratio, omitted item count, and truncation flag.

#### Scenario: Agent receives compact output accounting
- **WHEN** an agent invokes a compact MCP read response
- **THEN** the response includes `output_accounting.returned_token_estimate`, `original_token_estimate`, `estimated_tokens_saved`, `estimated_savings_ratio`, `omitted_count`, and `truncation_applied`
- **AND** the estimates are computed from response shape and byte size rather than LLM output

### Requirement: Track compact output metrics without content
Context accounting SHALL track compact-output usage through aggregate counters only. Persisted metrics SHALL NOT store queries, result snippets, prompts, note bodies, caller identity, or response bodies.

#### Scenario: Compact MCP output is counted
- **WHEN** a compact MCP response is served from a prepared repository runtime
- **THEN** context metrics increment compact-output counters and aggregate token estimates
- **AND** the stored metrics contain no query text or result content

#### Scenario: Existing metrics remain readable
- **WHEN** synrepo loads a context metrics file written before compact-output counters existed
- **THEN** missing compact-output fields default to zero
- **AND** the metrics file remains inspectable through text, JSON, and Prometheus surfaces

### Requirement: Track context flood metrics without content
Context accounting SHALL track aggregate response budget behavior without storing queries, snippets, prompts, note bodies, caller identity, or response bodies.

#### Scenario: Oversized MCP responses are counted
- **WHEN** an MCP response exceeds the soft cap
- **THEN** context metrics increment `responses_over_soft_cap_total`
- **AND** the response's estimated token count contributes to `tool_token_totals` for that tool

#### Scenario: Truncated MCP responses are counted
- **WHEN** the final response clamp trims a response
- **THEN** context metrics increment `responses_truncated_total`
- **AND** `largest_response_tokens` is updated when the response is the largest observed response

#### Scenario: Deep cards are counted
- **WHEN** a card-shaped response uses deep budget
- **THEN** context metrics increment `deep_cards_served_total`

#### Scenario: Context pack tokens are counted
- **WHEN** a context pack response is served
- **THEN** context metrics add the pack token estimate to `context_pack_tokens_total`

#### Scenario: Existing metrics remain readable
- **WHEN** synrepo loads a context metrics file written before flood metrics existed
- **THEN** missing flood metric fields default to zero
- **AND** the metrics file remains inspectable through JSON, text, and Prometheus surfaces
