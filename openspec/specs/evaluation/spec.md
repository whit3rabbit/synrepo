## Purpose
Define the success metrics, anti-metrics, and benchmark conditions used to judge whether synrepo meets its product wedge and trust goals.

## Requirements

### Requirement: Define success metrics for the product wedge
synrepo SHALL define evaluation metrics that measure time to first useful result, task success improvement, token savings, and wrong-file edit reduction against the stated product wedge.

#### Scenario: Judge a release candidate
- **WHEN** a milestone is evaluated for usefulness
- **THEN** the evaluation spec provides measurable success criteria tied to real agent outcomes
- **AND** the project can reject a release that lacks those outcomes even if component work shipped

### Requirement: Define trust and behavior anti-metrics
synrepo SHALL define anti-metrics and behavior metrics that track overlay reliance, contradiction rates, budget escalation, and other indicators of trust-model failure.

#### Scenario: Detect harmful product behavior
- **WHEN** the system appears to work functionally but agents rely on the wrong sources
- **THEN** the evaluation spec provides metrics that reveal the trust failure
- **AND** the product can treat it as a regression instead of a hidden success

### Requirement: Define realistic benchmark conditions
synrepo SHALL define benchmark conditions that reflect ugly repositories, incremental rebuilds, and real operational constraints rather than idealized demos.

#### Scenario: Evaluate on a messy repository
- **WHEN** a benchmark or demo is prepared
- **THEN** the evaluation contract requires realistic repository conditions
- **AND** success claims cannot rely solely on tidy showcase inputs

### Requirement: Define operational telemetry separate from graph truth
synrepo SHALL track operational metrics in a store that is physically separate from the canonical graph and overlay stores, so runtime performance and health are operator-visible without contaminating the trust model.

Metrics to track:
- structural compile durations (per-pass and total)
- reconcile outcomes and counts (completed, lock-conflict, failed) over time
- graph query counts by type (Phase 2+)
- stale-repair counts from reconcile passes
- token budget spent per card tier (Phase 2+)
- agent-facing request hit/miss rates against the graph vs. overlay (Phase 2+)

This store is for operator visibility and system health monitoring. It must never be read by the explain pipeline or used as input to graph production.

#### Scenario: Inspect compile performance over time
- **WHEN** an operator wants to understand whether the structural compile is keeping up with repository churn
- **THEN** the telemetry store provides a historical record of compile durations and reconcile outcomes
- **AND** this record is accessible from CLI diagnostics without touching the canonical graph

#### Scenario: Verify separation from graph truth
- **WHEN** the telemetry store is read by any pipeline component
- **THEN** the read must be rejected at the retrieval layer with an explicit boundary violation
- **AND** telemetry data must never appear as provenance or epistemic input to any graph node or edge

### Requirement: Require benchmark-backed context claims
synrepo SHALL only make numeric context-savings claims when backed by reproducible benchmark output.

#### Scenario: README reports context savings
- **WHEN** documentation includes a numeric context-savings percentage
- **THEN** the claim cites benchmark dimensions including reduction ratio, target hit rate, stale rate, latency, and test-link coverage
- **AND** unbenchmarked wording stays qualitative
