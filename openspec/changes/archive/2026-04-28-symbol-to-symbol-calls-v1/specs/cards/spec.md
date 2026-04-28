## MODIFIED Requirements

### Requirement: Budget tiers implement a progressive-disclosure protocol

synrepo's three budget tiers (`tiny`, `normal`, `deep`) SHALL be treated as a deliberate three-surface interaction pattern, not merely a size knob. Agents SHALL begin with `tiny` or `normal` to orient, and escalate to `deep` only when a specific field requires it. The card compiler SHALL maintain this contract across all card types: `tiny` returns index-quality signals, `normal` returns neighborhood-quality context (signature, neighbors, co-change partners), and `deep` returns inspection-quality detail (source body, full overlay content, full neighbor cards).

#### Scenario: SymbolCard deep budget includes graph callers and callees

- **WHEN** a `SymbolCard` is requested at `deep` budget for a symbol with symbol-to-symbol `Calls` edges
- **THEN** `callers` includes inbound caller symbols from graph-backed `Calls` edges
- **AND** `callees` includes outbound callee symbols from graph-backed `Calls` edges

#### Scenario: SymbolCard tight budgets withhold graph callers and callees

- **WHEN** a `SymbolCard` is requested at `tiny` or `normal` budget
- **THEN** `callers` and `callees` SHALL remain omitted or empty
- **AND** callers can escalate to `deep` when full symbol call neighbors are needed
