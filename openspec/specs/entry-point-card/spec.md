## Purpose
Define the EntryPointCard contract, the EntryPointKind taxonomy, heuristic detection rules, and budget-tier behavior for execution-root context packets.

## Requirements

### Requirement: Define EntryPointCard as a graph-derived card type
synrepo SHALL define `EntryPointCard` as a structured card that identifies execution roots within a scope using heuristic detection against graph-observed symbol names, file paths, and `SymbolKind` values. All detected entry points SHALL be sourced from `parser_observed` or `git_observed` graph facts. No LLM involvement and no overlay content SHALL appear in an `EntryPointCard`. Detection SHALL produce no result rather than a low-confidence spurious result when patterns do not match.

#### Scenario: Compile an EntryPointCard for a binary crate
- **WHEN** `entry_point_card(scope, budget)` is called on a repo containing a file at `src/main.rs` or `src/bin/*.rs` with a symbol named `main`
- **THEN** the returned card includes an entry for that symbol with `kind: "binary"`
- **AND** the entry records the `SymbolNodeId`, qualified name, and file-relative location
- **AND** `source_store` is `"graph"`

#### Scenario: Compile an EntryPointCard when no entry points match
- **WHEN** `entry_point_card(scope, budget)` is called and no symbols match any `EntryPointKind` pattern within the scope
- **THEN** the returned card has an empty `entry_points` list
- **AND** no error is raised

### Requirement: Define the EntryPointKind taxonomy
synrepo SHALL classify detected entry points into exactly four kinds. Each `EntryPoint` record SHALL carry exactly one `kind` value. Rules SHALL be applied in the order listed below; the first matching rule wins.

| Kind | Detection rule |
|---|---|
| `binary` | `qualified_name == "main"` and file path is `src/main.rs` or matches `src/bin/*.rs` |
| `cli_command` | File path contains a segment matching `cli`, `command`, or `cmd`, and symbol is a top-level function with `SymbolKind::Function` |
| `http_handler` | Symbol name matches the prefix `handle_`, `serve_`, or `route_`, or file path contains a segment matching `handler`, `route`, or `router` |
| `lib_root` | File path is `src/lib.rs` or a `mod.rs` at a module boundary, and the symbol is a top-level `pub fn` or `pub struct` with no callers from within the same file |

#### Scenario: Classify a main function as binary kind
- **WHEN** a symbol with `qualified_name == "main"` is found in `src/main.rs`
- **THEN** its `EntryPointKind` is `binary`
- **AND** no other kind is assigned to the same entry

#### Scenario: Classify a handler function by name prefix
- **WHEN** a symbol named `handle_request` exists in any file
- **THEN** its `EntryPointKind` is `http_handler`
- **AND** the entry is included in the card even if the file path does not contain `handler`

#### Scenario: Stop at first matching rule
- **WHEN** a symbol named `handle_command` exists in a file at `src/cli/handler.rs`
- **THEN** its `EntryPointKind` is `cli_command` (the `cli` path segment matches the second rule before the `handle_` prefix matches the third rule)
- **AND** only one entry is emitted for that symbol

#### Scenario: Omit unmatched symbols
- **WHEN** a symbol does not match any of the four detection rules
- **THEN** no `EntryPoint` record is emitted for that symbol
- **AND** the card is returned without error

### Requirement: Apply budget-tier truncation to EntryPointCard
synrepo SHALL truncate `EntryPointCard` content per the requested budget tier.

#### Scenario: Return a tiny EntryPointCard
- **WHEN** an `EntryPointCard` is requested at `tiny` budget
- **THEN** each entry includes only `kind`, `qualified_name`, and file-relative location (path + line)
- **AND** call-site count, doc comment, and source body are omitted

#### Scenario: Return a normal EntryPointCard
- **WHEN** an `EntryPointCard` is requested at `normal` budget
- **THEN** each entry additionally includes the count of unique callers in the graph and the doc comment truncated to 80 characters

#### Scenario: Return a deep EntryPointCard
- **WHEN** an `EntryPointCard` is requested at `deep` budget
- **THEN** each entry additionally includes the full one-line signature and, when a `SymbolCard` can be compiled for the entry point, it is inlined
