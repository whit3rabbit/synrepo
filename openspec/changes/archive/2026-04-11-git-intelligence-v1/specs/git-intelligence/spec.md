## MODIFIED Requirements

### Requirement: Define git-derived facts as secondary authority
synrepo SHALL mine repository history into deterministic `git_observed` evidence for ownership, hotspots, co-change, last meaningful change, and churn-aware ranking while keeping parser-observed structure as the stronger authority for current code behavior.

#### Scenario: Rank likely edit targets with history
- **WHEN** synrepo ranks files or symbols for a task
- **THEN** git-derived signals may improve ordering and explanation
- **AND** they do not override parser-observed structure or human-declared rationale

### Requirement: Define git-intelligence outputs
synrepo SHALL expose Git-derived enrichments through existing routing and card surfaces, including co-change partners, last meaningful change summaries, and ownership or hotspot hints.

#### Scenario: Inspect a file with recent churn
- **WHEN** a user or agent reads a card or routing result for a frequently changed file
- **THEN** the response may include declared Git-derived enrichment fields
- **AND** those fields are distinguishable as history-backed evidence rather than canonical code facts

### Requirement: Define degraded history behavior
synrepo SHALL detect shallow clones, detached HEADs, missing history depth, ignored submodules, and similar incomplete history states and degrade Git intelligence explicitly.

#### Scenario: Use Git intelligence in a shallow clone
- **WHEN** synrepo cannot mine enough history for its normal Git-intelligence pass
- **THEN** Git-derived outputs are marked degraded or omitted according to the contract
- **AND** synrepo does not fabricate stable-seeming ownership or hotspot results
