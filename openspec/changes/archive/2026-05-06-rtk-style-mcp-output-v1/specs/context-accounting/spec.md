## ADDED Requirements

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
