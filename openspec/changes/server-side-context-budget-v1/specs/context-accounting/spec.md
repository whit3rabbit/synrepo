## ADDED Requirements

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
