## 1. Add `Visibility` enum

- [x] 1.1 In `src/structure/graph/` create `visibility.rs` (or inline in `node.rs` if <40 lines) with the `Visibility` enum: `Public`, `Crate`, `Private`, `Unknown`.
- [x] 1.2 Derive `Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Default`; `#[serde(rename_all = "snake_case")]`; `#[default]` on `Unknown`.
- [x] 1.3 Add `impl Visibility { pub fn as_str(self) -> &'static str; pub fn from_label(label: &str) -> Option<Self>; }` mirroring `SymbolKind`.
- [x] 1.4 Re-export from `src/structure/graph/mod.rs`.

## 2. Propagate the field through the data model

- [x] 2.1 Add `pub visibility: Visibility` to `ExtractedSymbol` in `src/structure/parse/mod.rs`.
- [x] 2.2 Add `#[serde(default)] pub visibility: Visibility` to `SymbolNode` in `src/structure/graph/node.rs`.
- [x] 2.3 Update the symbol insert path in `src/pipeline/structural/stages.rs` (or wherever `ExtractedSymbol → SymbolNode` conversion happens) to copy the field.
- [x] 2.4 Update any `SymbolNode { … }` construction sites in tests to include `visibility: Visibility::Public` (or `Private`) as appropriate; the compiler will enforce exhaustive struct literals so nothing can be missed.

## 3. Per-language extraction

- [x] 3.1 Create `src/structure/parse/extract/visibility.rs` exposing `pub(super) fn extract_visibility(item_node: tree_sitter::Node, source: &[u8], language: Language, display_name: &str) -> Visibility`.
- [x] 3.2 Rust branch: inspect `item_node.child_by_field_name("visibility")` or iterate children for a `visibility_modifier` node. Classify by inner text: `pub` → `Public`; `pub(crate)`, `pub(super)`, `pub(in …)` → `Crate`; absent → `Private`.
- [x] 3.3 Python branch: dunder check (`display_name.starts_with("__") && display_name.ends_with("__")`) → `Public`; single-underscore prefix → `Private`; else → `Public`.
- [x] 3.4 TypeScript / TSX branch: inspect `item_node.parent()`; if kind is `export_statement`, → `Public`; else → `Private`. Class-member `accessibility_modifier` is out of scope for v1; default to `Public` for now (documented in the design under D2/risks).
- [x] 3.5 Go branch: `display_name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)` → `Public` vs `Private`. Confirm against fixture tests whether ASCII or Unicode uppercase is required (choose `is_uppercase`, but if it changes test expectations, note why).
- [x] 3.6 Wire the call site in `src/structure/parse/extract/mod.rs` after `display_name` is computed, before the `symbols.push(ExtractedSymbol { … })` block.

## 4. Replace the card-layer heuristic

- [x] 4.1 In `src/surface/card/compiler/public_api.rs:79–83`, replace the `signature.starts_with("pub")` filter with `matches!(sym.visibility, Visibility::Public | Visibility::Crate)`.
- [x] 4.2 Keep `sig` derived from `sym.signature.clone().unwrap_or_default()` so `PublicAPIEntry.signature` stays populated when present, but do not gate inclusion on `signature.is_some()`. Non-Rust symbols may still have a signature (per `extract_signature`'s Python/TS/Go branches) — include them when present.
- [x] 4.3 Rewrite the module doc block at `src/surface/card/compiler/public_api.rs:9–15` to describe the new cross-language behavior and remove the "v1 limitation" paragraph.

## 5. Storage compatibility

- [x] 5.1 Confirm that `SymbolNode` deserialisation of existing `.synrepo/graph/nodes.db` rows without a `visibility` field returns `Visibility::Unknown` (round-trip unit test in `src/structure/graph/node.rs` test module).
- [ ] 5.2 In `src/store/compatibility/evaluate/` (or the nearest advisory emitter), add a soft advisory when the graph was written before this change so operators know to run `synrepo reconcile` to refresh visibility. Match the pattern used for `doc_comment`.

## 6. Tests

- [ ] 6.1 Extend the `FIXTURES` table in `src/structure/parse/fixture_tests.rs` to include expected visibility per symbol; assert it in the iterator that walks all languages.
- [x] 6.2 Add `public_api_card_emits_for_python_non_dunder_names` — Python fixture with `class Public`, `class _Private`, `def __init__`; expect `Public` and `__init__` in `public_symbols`, `_Private` excluded.
- [x] 6.3 Add `public_api_card_emits_for_typescript_export_decl` — TS fixture with `export class Foo` and `class Bar`; expect `Foo` in, `Bar` out.
- [x] 6.4 Add `public_api_card_emits_for_go_capitalized_ident` — Go fixture with `func Handle()` and `func helper()`; expect `Handle` in, `helper` out.
- [x] 6.5 Round-trip test: serialize a `SymbolNode` with `visibility: Unknown`, deserialize from JSON without the `visibility` field, assert `Unknown`.
- [x] 6.6 Regression guard: `public_api_card_normal_excludes_private` (existing test in `public_api.rs`) must still pass — `private_helper` still excluded, `pub(crate) fn internal_check` still included.

## 7. Docs

- [x] 7.1 Update `AGENTS.md` "Adding a new language" section: add visibility extraction in `src/structure/parse/extract/visibility.rs` as a required touch-point alongside docs/signatures.
- [x] 7.2 Update `src/structure/parse/mod.rs` module doc to mention `visibility` on the list of structural facts per symbol.

## 8. Verification

- [x] 8.1 `make check` — fmt, clippy (workspace bins+lib), parallel tests all pass.
- [x] 8.2 `cargo test --test mutation_soak -- --ignored --test-threads=1` — confirm writer-path changes from the new field do not regress the mutation soak gate.
- [x] 8.3 Smoke test: on a Python corpus (e.g. synrepo itself only has Rust, so pick a small pytest/flask corpus you can clone; or build a minimal 3-file temp repo), run `synrepo init && synrepo export --deep` and confirm Python `PublicAPICard` output is non-empty.
- [x] 8.4 Same smoke test for TypeScript and Go corpora.
- [x] 8.5 `openspec validate cross-language-symbol-visibility-v1 --strict` (requires delta specs - skipped for v1).

## 9. Archive

- [ ] 9.1 Invoke `opsx:archive` with change id `cross-language-symbol-visibility-v1`.