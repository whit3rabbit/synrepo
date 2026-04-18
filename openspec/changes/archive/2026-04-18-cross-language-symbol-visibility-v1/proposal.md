## Why

`PublicAPICard` compilation in `src/surface/card/compiler/public_api.rs:79–83` decides visibility with a Rust-only string-prefix check:

```rust
let sig = match &sym.signature {
    Some(s) if s.starts_with("pub") => s.clone(),
    _ => continue,
};
```

The file's own module doc (lines 9–15) acknowledges the limitation: Python, TypeScript, and Go produce no public symbols via this path because none of them express visibility with a `pub` keyword prefix on the signature. The consequence is that `synrepo_public_api` cards for non-Rust directories return `public_symbol_count = 0` for directories that clearly have a public API.

`SymbolNode` (`src/structure/graph/node.rs:103–142`) has no `visibility` field. `ExtractedSymbol` (`src/structure/parse/mod.rs:60–75`) likewise lacks one. The parser has all the information it needs — tree-sitter captures the keyword or modifier at extraction time — but the downstream card layer has no structured data to consume and falls back to string heuristics.

This change promotes visibility to a first-class structural field so the surface layer stops reinventing language-specific parsers and so non-Rust corpora actually produce public-API cards.

## What Changes

- Introduce `Visibility` enum in `src/structure/graph/` with variants:
  - `Public` — exported / externally callable (`pub` in Rust, `export` in TS, underscore-free top-level name in Python, capitalized identifier in Go).
  - `Crate` — visible to the same compilation unit (`pub(crate)` / `pub(super)` / `pub(in …)` in Rust; no direct analogue in other languages → emitted only by Rust extraction).
  - `Private` — file/module-scoped (Rust default, Python `_name`, TS non-`export`, Go lowercase).
  - `Unknown` — extraction could not determine (reserved; emitted only during language bootstrap before the visibility path is wired).
- Add `visibility: Visibility` field to `ExtractedSymbol` and `SymbolNode`.
- Per-language extraction logic in `src/structure/parse/extract/` (new `visibility.rs` submodule):
  - **Rust**: inspect the declaration's `visibility_modifier` child node; `pub` → `Public`, `pub(crate|super|in …)` → `Crate`, absent → `Private`.
  - **Python**: `Private` if the `display_name` starts with `_` (excluding dunder names like `__init__` which are public protocol), otherwise `Public`. No `Crate` variant.
  - **TypeScript / TSX**: walk the declaration's ancestor chain to find an `export_statement` wrapper; present → `Public`, absent → `Private`. No `Crate` variant.
  - **Go**: first character of `display_name` is uppercase ASCII → `Public`, otherwise `Private`. No `Crate` variant.
- Persist the new field via SQLite JSON blob (`symbols.data` already encodes via serde; `#[serde(default)]` on the new field keeps existing rows readable).
- Storage-compatibility: rows written before the migration deserialize as `Visibility::Unknown` (via `#[serde(default)]` with a `Default` impl returning `Unknown`). No schema migration is required because the field is serialized into the existing JSON blob.
- Replace the `signature.starts_with("pub")` filter in `public_api.rs:79–83` with a check against the new field: accept `Visibility::Public` and `Visibility::Crate` (the card historically included `pub(crate)`); drop `Private` and `Unknown`.
- Add per-language fixture tests in `src/structure/parse/fixture_tests.rs` that assert the visibility of every fixture symbol.
- Add a `public_api_card_emits_for_non_rust_languages` test that bootstraps a Python, TS, and Go source tree and asserts non-empty `public_symbols`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `graph`: `SymbolNode` gains a `visibility` field; graph consumers that deserialize symbols must tolerate the new field (serde `#[serde(default)]` handles older rows).
- `cards`: `PublicAPICard` now emits entries for Python, TypeScript, and Go. The `public_symbols` count stops being zero for non-Rust directories that have a public API.
- `substrate` → `structure`: `ExtractedSymbol` gains `visibility`; all per-language extractors must populate it.

## Impact

- **Code**:
  - `src/structure/graph/node.rs` — add `visibility` field to `SymbolNode`; add `Visibility` enum (or extract to `src/structure/graph/visibility.rs` if it grows beyond ~40 lines).
  - `src/structure/parse/mod.rs` — add `visibility` field to `ExtractedSymbol`.
  - `src/structure/parse/extract/visibility.rs` — new submodule with per-language `extract_visibility(item_node, source, language, display_name) -> Visibility`.
  - `src/structure/parse/extract/mod.rs` — call the new extractor during the symbol loop (after `display_name` is known but before push).
  - `src/structure/parse/fixture_tests.rs` — extend the `FIXTURES` table entries to carry expected `Visibility` per symbol; add a new assertion loop.
  - `src/pipeline/structural/stages.rs` or wherever `ExtractedSymbol` is consumed into `SymbolNode` — propagate the field.
  - `src/surface/card/compiler/public_api.rs:79–83` — replace the string heuristic with `matches!(sym.visibility, Visibility::Public | Visibility::Crate)`.
  - `src/surface/card/compiler/public_api.rs:9–15` — remove the stale "v1 limitation" doc block.
- **Storage**: No SQLite schema migration (additive JSON field with `#[serde(default)]`). Symbols written before this change deserialize with `visibility = Unknown`; they will be excluded from `PublicAPICard` until their file is re-parsed on the next reconcile. This matches how existing symbols are refreshed for any parser change.
- **APIs**: No public API break. Card output format is unchanged (`PublicAPIEntry` fields already match). MCP tool `synrepo_public_api` gains non-zero results for non-Rust corpora — users perceive this as a bug fix.
- **Dependencies**: None. Tree-sitter queries already capture the nodes we need; this is a pure extraction refinement.
- **Docs**:
  - `AGENTS.md` — update the "Adding a new language" section to include visibility extraction as a required touch-point in the new `visibility.rs` submodule.
  - `src/surface/card/compiler/public_api.rs` doc block — rewrite to reflect cross-language support.
- **Systems**: This change unblocks #7 (`stage4-call-scope-narrowing-v1`), which will use visibility to inform call-resolution scope decisions (a short-name candidate that is `Private` is not callable from outside its file).
