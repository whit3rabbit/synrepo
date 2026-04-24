## ADDED Requirements

### Requirement: Require fixture evidence for numeric context-savings claims
Synrepo SHALL only publish numeric context-savings claims when they are backed by a reproducible fixture benchmark report that includes usefulness and freshness dimensions.

#### Scenario: Documentation includes a savings percentage
- **WHEN** README, release notes, or product docs state a numeric context-savings percentage
- **THEN** the claim cites or names a benchmark run that reports reduction ratio, target hit rate, miss rate, stale rate, latency, and test-link coverage when applicable
- **AND** the claim does not rely on token reduction alone

#### Scenario: No benchmark evidence exists
- **WHEN** no fixture-backed benchmark report exists for a context-savings statement
- **THEN** documentation uses qualitative wording such as bounded structural cards instead of a numeric savings percentage

### Requirement: Define minimum benchmark fixture coverage
The context benchmark fixture set SHALL cover more than one workflow category so release evidence is not based on a single happy-path query.

#### Scenario: Benchmark fixture set is reviewed
- **WHEN** a context benchmark fixture set is used for a release or README claim
- **THEN** it includes tasks for route-to-edit, symbol explanation, impact or risk, and test-surface discovery when those surfaces are supported by the repo under test
- **AND** missing categories are reported as gaps rather than silently ignored
