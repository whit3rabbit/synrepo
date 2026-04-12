## ADDED Requirements

### Requirement: Define DecisionCard as an optional rationale output
synrepo SHALL define DecisionCard as an optional card type returned when a queried node has incoming `Governs` edges from ConceptNodes with rationale content. DecisionCard is backed exclusively by `HumanDeclared` or `ParserObserved` ConceptNodes; overlay content SHALL NOT populate DecisionCard fields. The card SHALL distinguish rationale from current code behavior by labeling its source as human-authored.

#### Scenario: Return a DecisionCard when rationale exists
- **WHEN** an agent queries a node that has incoming Governs edges from one or more ConceptNodes
- **THEN** the response MAY include a DecisionCard containing the decision title, status (if available), decision text, and the IDs of governed nodes
- **AND** the DecisionCard source is labeled as human-authored, not as structural observation

#### Scenario: No DecisionCard when no rationale is linked
- **WHEN** an agent queries a node with no incoming Governs edges
- **THEN** no DecisionCard is included in the response
- **AND** the structural card is returned unchanged

#### Scenario: DecisionCard does not override structural truth
- **WHEN** a DecisionCard describes a design decision that conflicts with observed code behavior
- **THEN** the structural card fields reflect current observed code state
- **AND** the DecisionCard content is labeled as rationale, not as a code fact
- **AND** no structural field is modified to match the DecisionCard content

### Requirement: Define DecisionCard budget tier behavior
synrepo SHALL apply the same `tiny` / `normal` / `deep` budget tier model to DecisionCard as to other card types. At `tiny` tier, DecisionCard includes only the decision title and governed node IDs. At `normal` tier, it adds status and a truncated decision body. At `deep` tier, it includes the complete decision body and all linked ConceptNode IDs.

#### Scenario: Return a tiny DecisionCard
- **WHEN** a tool requests a `tiny` budget response for a node with linked rationale
- **THEN** the DecisionCard includes only title and governed node IDs
- **AND** the decision body is omitted
