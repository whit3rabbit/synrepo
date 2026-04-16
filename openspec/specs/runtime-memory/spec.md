# runtime-memory spec

## MODIFIED Requirements

### Requirement: Bounded-memory top-k vector query

Vector similarity query MUST scale memory with requested top-k, not with total
chunk count, except for the stored index itself.

#### Scenario: querying a large embedding index
- **WHEN** a query requests the top 20 matches from a large embedding index
- **THEN** the implementation MUST NOT allocate a score entry for every chunk
- **AND** it MUST preserve the same returned ranking semantics as brute-force
  top-k cosine selection

### Requirement: Store-side limited overlay review

Overlay review/list surfaces MUST apply ordering and limit at the store layer
when the backing store supports it.

#### Scenario: limited review queue
- **WHEN** the caller requests the top 50 review-queue candidates
- **THEN** the store MUST return candidates in descending review order
- **AND** the runtime MUST NOT load the entire candidate set only to truncate it

### Requirement: Aggregate-first status reporting

Status/report surfaces SHOULD prefer aggregate queries over full row scans when
the user-visible output only requires counts, freshness summaries, or bounded
samples.

#### Scenario: commentary/status coverage summary
- **WHEN** a status surface only needs counts or freshness totals
- **THEN** the implementation SHOULD compute those values using aggregate or
  bounded queries where possible
- **AND** it SHOULD avoid loading all rows into memory unless required for
  correctness