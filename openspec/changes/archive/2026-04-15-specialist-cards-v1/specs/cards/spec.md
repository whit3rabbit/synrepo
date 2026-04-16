## MODIFIED Requirements

### Requirement: Define card types as the product surface
synrepo SHALL define card contracts for the core structural card types that agents use to orient, route edits, assess impact, and inspect test coverage. The card types are: `SymbolCard`, `FileCard`, `DecisionCard`, `EntryPointCard`, `ModuleCard`, `CallPathCard`, and `TestSurfaceCard`.

#### Scenario: Ask for context about a symbol
- **WHEN** an agent requests a symbol-focused context packet
- **THEN** the cards spec defines the required structural fields for the returned card type
- **AND** the response can be understood without reading arbitrary source files first
