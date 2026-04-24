## Purpose
Define the tree-sitter-backed structural parsing contract that produces deterministic `ParseOutput` (symbols, qualified names, signatures, doc comments, `call_refs`, `import_refs`) across the supported language set, with explicit query-validation, kind-mapping, and malformed-source contracts that CI enforces.

## Requirements

### Requirement: Query compilation is validated for every supported language

The system SHALL validate, in automated tests, that every supported parser language's embedded tree-sitter queries compile and expose the captures that downstream extraction depends on. A query that fails to compile or is missing a required capture SHALL cause the test suite to fail.

#### Scenario: Definition queries compile and expose required captures

- **WHEN** the parser validation test suite runs
- **THEN** every supported `Language` variant's definition query SHALL compile against its grammar without error
- **AND** the `item` and `name` captures SHALL be present in that query

#### Scenario: Call queries compile and expose required captures

- **WHEN** the parser validation test suite runs
- **THEN** every supported `Language` variant's call query SHALL compile against its grammar without error
- **AND** the `callee` capture SHALL be present for every language where call extraction is supported

#### Scenario: Import queries compile and expose required captures

- **WHEN** the parser validation test suite runs
- **THEN** every supported `Language` variant's import query SHALL compile against its grammar without error
- **AND** the `import_ref` capture SHALL be present for every language where import extraction is supported

#### Scenario: Missing or broken query fails CI loudly

- **WHEN** a query fails to compile or a required capture is removed
- **THEN** the parser validation test suite SHALL fail with a diagnostic identifying the language and the query role (definition, call, or import)

### Requirement: Symbol-kind mapping is pinned per language

The system SHALL pin the mapping from a query's pattern index to `SymbolKind` for every supported language in automated tests. Reordering patterns or removing a pattern without updating the mapping SHALL cause the test suite to fail.

#### Scenario: Pattern map matches query pattern ordering

- **WHEN** the parser validation test suite runs for a supported language
- **THEN** the pattern-index to `SymbolKind` mapping used at extraction time SHALL match the compiled query's actual pattern ordering
- **AND** any drift between the mapping and the query SHALL cause the test to fail

#### Scenario: Pattern-index coverage is exhaustive

- **WHEN** the parser validation test suite runs for a supported language
- **THEN** every pattern index emitted by that language's definition query SHALL have an explicit `SymbolKind` assignment in the test-pinned mapping
- **AND** no pattern index SHALL rely on an unpinned fallback to pass the test

### Requirement: Every supported language has explicit parser extraction coverage

The system SHALL provide parser fixture coverage for every supported language. Supported languages today are Rust, Python, TypeScript, TSX, and Go. Adding a new `Language` variant SHALL require adding fixtures for that variant before the test suite passes.

#### Scenario: Supported language fixture parses end-to-end

- **GIVEN** a fixture file for a supported language
- **WHEN** `parse_file` is invoked on the fixture
- **THEN** the returned `ParseOutput.language` SHALL match the fixture's language
- **AND** the expected symbols SHALL be extracted with the expected `SymbolKind` values
- **AND** expected qualified names, signatures, and doc comments SHALL match

#### Scenario: TSX has dedicated fixture coverage distinct from TypeScript

- **GIVEN** a `.tsx` fixture containing a JSX-bearing component
- **WHEN** `parse_file` is invoked on the fixture
- **THEN** the returned `ParseOutput.language` SHALL be `Language::Tsx`
- **AND** symbol extraction SHALL succeed without relying on TypeScript-only assumptions

#### Scenario: Fixture coverage enforces language enumeration

- **WHEN** a new `Language` variant is added without corresponding fixtures
- **THEN** the parser test suite SHALL fail until fixtures for the new variant are added

### Requirement: Call and import references are tested as first-class parser outputs

The system SHALL exercise `ParseOutput.call_refs` and `ParseOutput.import_refs` directly in automated tests, independent of stage-4 resolution. Parser regressions that degrade these fields SHALL fail tests even when stage 4 would silently skip unresolved references.

#### Scenario: Call references are extracted per language

- **GIVEN** a fixture with call sites for a supported language
- **WHEN** `parse_file` is invoked
- **THEN** each expected call reference SHALL be present in `ParseOutput.call_refs`
- **AND** each call reference SHALL carry the local call-site name expected by stage 4

#### Scenario: Import references are extracted per language

- **GIVEN** a fixture with imports for a supported language
- **WHEN** `parse_file` is invoked
- **THEN** each expected import reference SHALL be present in `ParseOutput.import_refs`
- **AND** each import reference SHALL carry the raw import path expected by stage 4 for that language

#### Scenario: Intentionally unsupported import forms are absent

- **GIVEN** an import form the parser intentionally does not extract for a language
- **WHEN** `parse_file` is invoked
- **THEN** `ParseOutput.import_refs` SHALL NOT contain an entry for that form
- **AND** the test suite SHALL document this as intentional

### Requirement: Qualified-name derivation is tested on edge cases

The system SHALL lock qualified-name derivation behavior on known-fragile constructs for every supported language that exposes that construct, so that future edits to ancestor-walking or type-stripping logic cannot silently regress.

#### Scenario: Rust generic impl methods are qualified correctly

- **GIVEN** a Rust fixture containing `impl<T> Foo<T> { fn bar(...) {} }`
- **WHEN** `parse_file` extracts symbols
- **THEN** the method's qualified name SHALL name the impl type without its generic parameters

#### Scenario: Rust trait impl methods are qualified correctly

- **GIVEN** a Rust fixture containing `impl Trait for Foo { fn bar(...) {} }`
- **WHEN** `parse_file` extracts symbols
- **THEN** the method's qualified name SHALL reflect the implementing type

#### Scenario: Nested scopes disambiguate same-name symbols

- **GIVEN** a fixture containing two same-named symbols in different scopes (e.g. nested modules, nested classes, or class methods)
- **WHEN** `parse_file` extracts symbols
- **THEN** each symbol's qualified name SHALL be distinct and reflect its enclosing scope

### Requirement: Malformed-source behavior is intentional

The system SHALL pin `parse_file` behavior for unsupported extensions, malformed-but-supported source, and empty inputs. Runtime SHALL remain permissive for ordinary malformed user source.

#### Scenario: Unsupported extension returns None

- **GIVEN** a file with an extension that has no wired grammar
- **WHEN** `parse_file` is invoked
- **THEN** it SHALL return `None`

#### Scenario: Malformed but supported source returns best-effort output

- **GIVEN** syntactically malformed source in a supported language
- **WHEN** `parse_file` is invoked
- **THEN** it SHALL NOT panic
- **AND** it SHALL return `Some(ParseOutput)` containing deterministic best-effort extraction
- **AND** it SHALL NOT escalate malformed source to a hard error

#### Scenario: Parse output is deterministic across runs

- **GIVEN** identical source bytes for a supported language
- **WHEN** `parse_file` is invoked repeatedly
- **THEN** the returned `ParseOutput` SHALL be identical across runs

### Requirement: Stage-4 integration tests lock the current approximate-resolution contract

The system SHALL validate, through integration tests that exercise `ParseOutput` consumers in stage 4, that parser-produced call and import references resolve according to the current documented contract. These tests SHALL NOT change the contract, they lock it in place.

#### Scenario: Ambiguous call name emits edges to all candidates

- **GIVEN** a call reference whose short name matches multiple symbols in the graph
- **WHEN** stage 4 resolves references
- **THEN** it SHALL emit a `Calls` edge to each matching candidate symbol

#### Scenario: Unresolved call or import is skipped without error

- **GIVEN** a call reference or import reference that cannot be resolved to any known symbol or file
- **WHEN** stage 4 resolves references
- **THEN** it SHALL skip the reference
- **AND** it SHALL NOT fail the compile

#### Scenario: TypeScript relative imports resolve to target files

- **GIVEN** a TypeScript or TSX source with a relative import path
- **WHEN** stage 4 resolves imports
- **THEN** the import SHALL resolve to the target file according to the current TypeScript resolution contract

#### Scenario: Python dotted imports resolve according to current rules

- **GIVEN** a Python source with a dotted import
- **WHEN** stage 4 resolves imports
- **THEN** the import SHALL resolve to a target file according to the current Python resolution contract

#### Scenario: Rust `use` paths resolve to in-crate module files

- **GIVEN** a Rust source with a `use` declaration whose path is prefixed by `crate::`, `self::`, or `super::`, or whose first segment matches a top-level in-crate module
- **WHEN** stage 4 resolves imports
- **THEN** the import SHALL resolve to the first existing candidate file among `<path>.rs` and `<path>/mod.rs`, evaluated against the importing file's crate root
- **AND** `super::` segments SHALL walk one directory upward per occurrence before applying the remainder

#### Scenario: Rust `use` paths pointing to sub-items resolve to the nearest enclosing module file

- **GIVEN** a Rust `use` path of the form `crate::module::Thing` where `Thing` is a symbol defined inside a file, not itself a file
- **WHEN** stage 4 resolves imports
- **THEN** stage 4 SHALL also attempt candidate files after dropping the last path segment (`<path_without_last>.rs`, `<path_without_last>/mod.rs`)
- **AND** it SHALL emit an `Imports` edge to the longest-matching existing candidate

#### Scenario: External Rust crate paths are skipped silently

- **GIVEN** a Rust `use` path whose first segment is a third-party or standard-library crate (e.g., `std::`, `serde::`) and whose candidates do not exist in the graph's file index
- **WHEN** stage 4 resolves imports
- **THEN** it SHALL skip that import without error

#### Scenario: Go imports resolve via `go.mod` module-prefix stripping

- **GIVEN** a Go source with an import path that begins with the module prefix declared by the repo's `go.mod`
- **WHEN** stage 4 resolves imports
- **THEN** stage 4 SHALL strip the module prefix and resolve the remainder as a repo-relative package directory
- **AND** it SHALL emit an `Imports` edge from the importing file to every `.go` file directly contained in that package directory

#### Scenario: Go imports without a matching `go.mod` prefix are skipped silently

- **GIVEN** a Go source with an import whose path does not match the local `go.mod` module prefix, or a repository with no readable `go.mod`
- **WHEN** stage 4 resolves imports
- **THEN** it SHALL skip the import without error

### Requirement: Map parser coverage failures to readiness
Structural parse diagnostics SHALL map parser failures and unsupported-language gaps into capability readiness rows.

#### Scenario: Parser failures occur during reconcile
- **WHEN** a reconcile or bootstrap pass records parser failures for supported files
- **THEN** the readiness matrix marks parser coverage as degraded
- **AND** the row includes failure counts and a next action that points to check, sync, or parser diagnostics

#### Scenario: Unsupported files are present
- **WHEN** files are unsupported by structural parsing but are otherwise admitted by the repo
- **THEN** the readiness matrix distinguishes unsupported coverage from parser failure
- **AND** unsupported coverage does not masquerade as parser-observed graph truth
