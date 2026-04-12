## ADDED Requirements

### Requirement: Extract symbol signature and doc comment during structural parse
synrepo SHALL extract a one-line signature and the immediately-preceding doc comment for each matched symbol node during structural parse, for all supported languages (Rust, Python, TypeScript/TSX), and SHALL persist the extracted values on the corresponding `SymbolNode`.

#### Scenario: Parse a documented Rust function
- **WHEN** the structural compile processes a Rust source file containing a `///`-commented function
- **THEN** the resulting `SymbolNode` has a non-None `signature` field containing the function declaration text up to the opening brace
- **AND** the `doc_comment` field contains the concatenated `///` comment lines immediately preceding the function

#### Scenario: Parse an undocumented symbol
- **WHEN** the structural compile processes a symbol that has no preceding doc comment
- **THEN** the resulting `SymbolNode` has `doc_comment: None`
- **AND** `signature` is still populated from the declaration text if the language supports it

#### Scenario: Parse a Python function with a docstring
- **WHEN** the structural compile processes a Python function whose body begins with a string literal
- **THEN** the `doc_comment` field on the resulting `SymbolNode` contains that string literal's text
- **AND** `signature` contains the `def` line up to and including the closing `:`

#### Scenario: Parse a TypeScript function with a JSDoc comment
- **WHEN** the structural compile processes a TypeScript function preceded by a `/** */` block comment
- **THEN** the `doc_comment` field on the resulting `SymbolNode` contains the JSDoc content
- **AND** `signature` contains the function declaration up to the opening brace
