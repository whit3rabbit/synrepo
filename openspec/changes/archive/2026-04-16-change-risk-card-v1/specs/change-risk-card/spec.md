## ADDED Requirements

### Requirement: ChangeRiskCard provides change risk assessment
synrepo SHALL define ChangeRiskCard as a computed card that aggregates drift score, co-change relationships, and git hotspot data into a risk assessment for a symbol or file target.

#### Scenario: Return ChangeRiskCard for a symbol with high risk
- **WHEN** an agent requests a ChangeRiskCard for a symbol that has high drift score, many co-change partners, and recent hotspot activity
- **THEN** the card returns risk_level: "high", drift_score >= 0.6, co_change_partner_count >= 5, and a non-empty risk_factors list

#### Scenario: Return ChangeRiskCard for a symbol with low risk
- **WHEN** an agent requests a ChangeRiskCard for a symbol with no drift edges, no co-change partners, and no recent touches
- **THEN** the card returns risk_level: "low", drift_score: 0, co_change_partner_count: 0, and empty risk_factors

#### Scenario: Return ChangeRiskCard with partial signals
- **WHEN** an agent requests a ChangeRiskCard for a symbol that has drift data but no co-change or hotspot data
- **THEN** the card returns available signals with null/missing for unavailable ones, and risk_factors lists only the available signals

### Requirement: ChangeRiskCard computes risk from graph signals only
ChangeRiskCard SHALL compute all risk signals from the graph store exclusively. It SHALL NOT query the overlay store or require LLM synthesis.

#### Scenario: ChangeRiskCard has no overlay dependency
- **WHEN** ChangeRiskCard is compiled
- **THEN** the card does not require any overlay store access
- **AND** all fields are derived from graph tables (edge_drift, edges, git_intelligence payloads)

### Requirement: synrepo_change_risk MCP tool returns ChangeRiskCard
synrepo SHALL provide a `synrepo_change_risk` MCP tool that accepts a symbol or file target and returns a ChangeRiskCard.

#### Scenario: MCP tool returns card for symbol
- **WHEN** an agent calls synrepo_change_risk with a symbol target (e.g., "src/main.rs::main")
- **THEN** the tool returns a ChangeRiskCard for that symbol

#### Scenario: MCP tool returns card for file
- **WHEN** an agent calls synrepo_change_risk with a file target (e.g., "src/main.rs")
- **THEN** the tool returns a ChangeRiskCard for that file

#### Scenario: MCP tool handles missing target
- **WHEN** an agent calls synrepo_change_risk with a non-existent symbol or file
- **THEN** the tool returns an error with a clear message

### Requirement: ChangeRiskCard implements budget tier behavior
ChangeRiskCard SHALL implement the same budget tier model as other card types. At `tiny` tier, only risk_level is returned. At `normal` tier, it adds risk_factors. At `deep` tier, it adds drift_score, co_change_partner_count, hotspot_score, and affected_revisions.

#### Scenario: Return tiny ChangeRiskCard
- **WHEN** a ChangeRiskCard is requested at `tiny` budget
- **THEN** the response includes only risk_level

#### Scenario: Return normal ChangeRiskCard
- **WHEN** a ChangeRiskCard is requested at `normal` budget
- **THEN** the response includes risk_level and risk_factors

#### Scenario: Return deep ChangeRiskCard
- **WHEN** a ChangeRiskCard is requested at `deep` budget
- **THEN** the response includes risk_level, risk_factors, drift_score, co_change_partner_count, hotspot_score, and affected_revisions

### Requirement: CLI surface for change risk
synrepo SHALL provide a CLI command `synrepo change-risk <target>` that returns a ChangeRiskCard for the specified symbol or file target.

#### Scenario: CLI returns card for valid target
- **WHEN** an agent runs `synrepo change-risk src/main.rs::main`
- **THEN** stdout includes a ChangeRiskCard in JSON or text format

#### Scenario: CLI handles missing target
- **WHEN** an agent runs `synrepo change-risk no/such/file.rs`
- **THEN** stdout includes an error message