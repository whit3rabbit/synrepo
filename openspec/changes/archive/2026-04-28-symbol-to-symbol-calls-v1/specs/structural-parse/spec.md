## MODIFIED Requirements

### Requirement: Stage-4 integration tests lock the current approximate-resolution contract

The system SHALL validate, through integration tests that exercise `ParseOutput` consumers in stage 4, that parser-produced call and import references resolve according to the current documented contract. These tests SHALL NOT change the contract, they lock it in place.

#### Scenario: Symbol-scoped call refs emit symbol-to-symbol Calls edges

- **GIVEN** a call reference whose call site is enclosed by an extracted function or method symbol
- **WHEN** stage 4 resolves the call to a positive-scoring callee symbol
- **THEN** it SHALL emit a `Calls` edge from the caller symbol to the callee symbol
- **AND** it MAY also emit the file-scoped `Calls` edge during the transition window

#### Scenario: Module-scope calls remain file-scoped

- **GIVEN** a call reference whose call site has no enclosing function or method symbol
- **WHEN** stage 4 resolves the call to a positive-scoring callee symbol
- **THEN** it SHALL emit the file-scoped `Calls` edge
- **AND** it SHALL NOT invent a synthetic caller symbol

#### Scenario: Caller body hash changes retire owned symbol call edges

- **GIVEN** stage 4 previously emitted a symbol-to-symbol `Calls` edge from a caller symbol
- **WHEN** the caller body changes and receives a new symbol identity
- **THEN** the old parser-owned `Calls` edge SHALL retire with the old caller observation
- **AND** compaction SHALL remove the retired observation after the configured retention window
