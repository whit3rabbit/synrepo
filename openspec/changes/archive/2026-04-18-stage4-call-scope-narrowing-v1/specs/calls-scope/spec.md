## ADDED Requirements

### Requirement: Scope-Narrowed Call Resolution

Stage 4 call resolver MUST apply scope scoring to emitted `Calls` edges, using these rules:
- Only emit edges when `top_score > 0`
- Emit uniquely-scored candidates OR ties at score >= 80
- Drop ties at score < 80

#### Scenario: Call to imported function
- **GIVEN** File A imports `file_b::transform` and contains a call to `transform()`
- **WHEN** the call resolver scores candidates
- **THEN** emit a single `Calls` edge from A to `file_b::transform`
- **AND** do NOT emit edges to other `transform` definitions in the codebase

#### Scenario: Call to private function from outside file
- **GIVEN** File A defines `fn private_helper()` and File B calls it
- **WHEN** the call resolver scores candidates
- **THEN** emit no `Calls` edge (private functions are not reachable cross-file)

#### Scenario: Ambiguous short name with no scope hints
- **GIVEN** Two unrelated modules define `map`, a third file with no relevant imports calls `map()`
- **WHEN** the call resolver scores candidates
- **THEN** emit no `Calls` edge (weak ambiguity case, score < 80)

### Requirement: Scoring Rubric Signals

The call resolver MUST apply these score signals:

| Signal | Score |
|--------|-------|
| Same file | +100 |
| Imported file (direct or module-level) | +50 |
| `visibility == Public` | +20 |
| `visibility == Crate` | +10 |
| `visibility == Private` cross-file | -100 |
| `is_method` AND candidate `kind == Method` | +30 |
| Free call AND candidate `kind ∈ {Function, Constant}` | +30 |
| `callee_prefix` matches candidate's qualified name segment | +40 |

#### Scenario: Same file with public visibility
- **GIVEN** File A defines `pub fn process(data: Data)` and calls `process()`
- **WHEN** the call resolver scores the candidate
- **THEN** score = +100 (same file) + 20 (public) = 120
- **AND** the edge is emitted

#### Scenario: Prefix match on imported function
- **GIVEN** File A imports `HashMap` and calls `HashMap::new()`, candidate is `crate::map::Map::new`
- **WHEN** the call resolver scores the candidate
- **THEN** the score includes +40 for prefix match
- **AND** the edge is emitted if total score >= 80