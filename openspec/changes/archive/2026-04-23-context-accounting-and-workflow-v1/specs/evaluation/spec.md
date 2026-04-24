## ADDED Requirements

### Requirement: Require benchmark-backed context claims
synrepo SHALL only make numeric context-savings claims when backed by reproducible benchmark output.

#### Scenario: README reports context savings
- **WHEN** documentation includes a numeric context-savings percentage
- **THEN** the claim cites benchmark dimensions including reduction ratio, target hit rate, stale rate, latency, and test-link coverage
- **AND** unbenchmarked wording stays qualitative
