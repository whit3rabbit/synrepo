## Purpose
Define how synrepo uses repository history as non-canonical input for routing, impact analysis, and change-risk enrichment.

## Requirements

### Requirement: Define git-derived facts as secondary authority
synrepo SHALL treat ownership, hotspots, co-change, last meaningful change, churn, and related history signals as `git_observed` facts that enrich routing without becoming canonical descriptive truth over parser-observed code facts.

#### Scenario: Use history to rank edit targets
- **WHEN** a user asks where to edit or what might break
- **THEN** git-derived signals may influence ranking and explanation
- **AND** they do not override parser-observed structure or human-declared rationale

### Requirement: Define git-intelligence outputs
synrepo SHALL define which card fields and MCP responses may expose git-derived enrichments, including ownership hints, hotspot signals, co-change partners, and last meaningful change summaries.

#### Scenario: Read a change-risk card
- **WHEN** a card includes git-derived enrichment
- **THEN** the contract identifies the history-backed fields and their authority
- **AND** the user can distinguish them from graph structure and overlay commentary

### Requirement: Define degraded history behavior
synrepo SHALL define how git intelligence behaves on shallow clones, detached HEADs, missing history, ignored submodules, and other degraded repository states.

#### Scenario: Run on a shallow clone
- **WHEN** the repository does not provide enough history for normal git mining
- **THEN** synrepo reports degraded git intelligence rather than inventing stable-seeming results
- **AND** structural graph behavior continues without requiring full history
