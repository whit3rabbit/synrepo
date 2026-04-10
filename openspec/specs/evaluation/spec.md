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
