## MODIFIED Requirements

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
