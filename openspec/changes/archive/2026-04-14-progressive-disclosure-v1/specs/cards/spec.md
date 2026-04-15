## ADDED Requirements

### Requirement: Budget tiers implement a progressive-disclosure protocol
synrepo's three budget tiers (`tiny`, `normal`, `deep`) SHALL be treated as a deliberate three-surface interaction pattern, not merely a size knob. Agents SHALL begin with `tiny` or `normal` to orient, and escalate to `deep` only when a specific field requires it. The card compiler SHALL maintain this contract across all card types: `tiny` returns index-quality signals, `normal` returns neighborhood-quality context (signature, neighbors, co-change partners), and `deep` returns inspection-quality detail (source body, full overlay content, full neighbor cards).

#### Scenario: Agent orients with tiny budget first
- **WHEN** an agent begins work on an unfamiliar symbol or file
- **THEN** a `tiny` budget request returns enough signal (name, kind, location, edge counts) to decide whether to escalate
- **AND** no source body, overlay commentary, or neighbor detail is included at `tiny` budget

#### Scenario: Agent escalates from tiny to normal
- **WHEN** an agent determines a symbol is relevant after a `tiny`-budget response
- **THEN** a `normal` budget request adds signature, doc comment, co-change partners, and structural neighbor summaries
- **AND** source body and full overlay content remain absent at `normal` budget

#### Scenario: Agent escalates to deep only for inspection
- **WHEN** an agent needs to read or edit a symbol's implementation
- **THEN** a `deep` budget request adds source body, full overlay commentary, full neighbor cards, and proposed cross-links
- **AND** the escalation is explicit — callers do not receive `deep` content unless they request it

#### Scenario: Budget tier is preserved in the response
- **WHEN** any card is returned by the card compiler or an MCP tool
- **THEN** the response includes a field identifying the budget tier used
- **AND** callers can distinguish a `tiny`-budget response from a `normal` or `deep` response without inspecting field presence
