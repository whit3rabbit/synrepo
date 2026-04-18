## MODIFIED Requirements

### Requirement: Cross-link span verification handles large sources
Cross-link verification in `src/pipeline/repair/cross_link_verify.rs` SHALL use a three-stage cascade to verify cited spans: (1) exact substring match on normalized text for verbatim citations, (2) anchored partial match with LCS verification for paraphrases, and (3) budgeted windowed LCS fallback. This enables verification of spans in sources larger than 4KB without silent drops.

#### Scenario: Verify verbatim citation in 10KB source
- **GIVEN** a cross-link with a cited span of 200 bytes in a source file of 10KB
- **WHEN** `verify_candidate_payload` validates the link
- **THEN** Stage A (exact substring) finds the span immediately with ratio = 1.0
- **AND** the function returns a `CitedSpan` with `lcs_ratio = 1.0` and `verified_at_offset` pointing to the found location

#### Scenario: Verify paraphrase in large source
- **GIVEN** a cross-link with a cited span that differs from the source by one word
- **WHEN** Stage A misses (not exact match) but Stage B anchor is found
- **THEN** Stage B evaluates LCS on a window around each anchor hit
- **AND** returns ratio >= 0.9 if found, otherwise falls through to Stage C

#### Scenario: Handle pathological large source with budget
- **GIVEN** a 500KB source with a needle that doesn't match
- **WHEN** stages A and B both fail to find a >= 0.9 ratio match
- **THEN** Stage C runs with a 50ms time budget
- **AND** returns the best-so-far ratio found before budget trip, or None if none evaluated
- **AND** emits a `tracing::warn!` with source length, needle length, and best ratio

### Requirement: Logging for verification stage decisions
The cross-link verification path SHALL emit structured logging to enable observability of which stage in the cascade produced a match and when budget trips occur.

#### Scenario: Log stage cascade decision
- **GIVEN** a span verification request
- **WHEN** a match is found in any stage
- **THEN** emit `tracing::debug!(stage = "A"|"B"|"C", ratio, message)` with the stage identifier

#### Scenario: Log budget trip
- **GIVEN** a verification that exceeds the time budget in Stage B or Stage C
- **WHEN** the budget check triggers
- **THEN** emit `tracing::warn!` with stage, source length, needle length, anchor hits (Stage B), iterations (Stage C), and best ratio if found