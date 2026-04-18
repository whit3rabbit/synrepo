## Context

`PublicAPICard` is the entrypoint that agents use to probe the exported surface of a directory. Today it is effectively Rust-only because it matches `signature.starts_with("pub")`. Python, TS, and Go parse fine and produce symbols, signatures, and doc comments, but every symbol is discarded by the visibility filter.

The fix has two layers:

1. **Structural** — make visibility a first-class property of `SymbolNode`, so the graph stores what the parser actually observed.
2. **Surface** — teach `PublicAPICard` to read the structured field.

Doing only (2) would mean reproducing per-language parsing logic in the card compiler, which is the wrong layer. Doing only (1) without updating the card would leave the field unused. This proposal does both in the same change.

## Goals / Non-Goals

**Goals:**

- Produce `PublicAPICard` output for Python, TypeScript/TSX, and Go with semantics that match each language's natural visibility notion.
- Give the graph a structured `visibility` field on symbols so downstream consumers (future: call-scope narrowing, neighborhood expansion, embedding recall filters) do not re-derive it.
- Keep the change additive at the storage layer. Symbols written before this change still load; they simply return `Unknown` until their file is re-parsed.

**Non-Goals:**

- No refactor of `PublicAPIEntry` or card token budgets.
- No change to `EntryPointCard` classification, even though it shares the card module. Entry points are orthogonal.
- No language-specific visibility semantics beyond the four listed here. Protected/internal keywords (C#, Java) can be added when those grammars are wired.
- No change to the card's Rust behavior — a symbol that is `pub(crate)` today is still included in `PublicAPIEntry` after the change.

## Decisions

### D1: `Visibility` enum with four variants, not a bool

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Crate,
    Private,
    #[default]
    Unknown,
}
```

A bool (`is_public`) would conflate Rust's `pub` and `pub(crate)` into one label, which loses information and would need to be un-encoded later. An enum keeps the door open for Java/C# protected variants without another schema change.

`Unknown` is the serde default because pre-migration symbols have no visibility field in their JSON blob. They will be refreshed on the next reconcile that re-parses their owning file. This is the same migration pattern we use for every other additive symbol field.

**Rationale**: the enum is open-ended enough for real semantics but not so wide that cross-language card logic has to handle 10 variants. Rust-specific `pub(super)` and `pub(in path)` collapse to `Crate` because the `PublicAPICard` consumer treats them identically today (both pass the `starts_with("pub")` check).

### D2: Visibility is populated at extraction, not at card-compile time

The parser already has the tree-sitter node for each symbol. Visibility is a one-pass inspection of that node. Computing it during card compile would mean reopening the source file or reparsing the signature string, both of which are strictly worse than reading a structural field.

Per-language extraction strategy (in `src/structure/parse/extract/visibility.rs`):

- **Rust**: `item_node.child_by_field_name("visibility")` or walk `item_node.children()` looking for a `visibility_modifier` node (tree-sitter-rust exposes these for `fn`, `struct`, `enum`, `trait`, `impl`, `const`, `static`, `mod`, `type`). Inspect the token text: `pub` bare → `Public`; `pub(crate)`, `pub(super)`, `pub(in …)` → `Crate`; no node → `Private`.
- **Python**: no AST-level visibility — the convention is `_name` for private. Check `display_name`: starts with `_` and is not a dunder (`__name__`) → `Private`; otherwise `Public`. Dunders (`__init__`, `__repr__`, etc.) are protocol and therefore `Public`.
- **TypeScript / TSX**: tree-sitter-typescript emits `export_statement` as a parent wrapper around exported declarations. Walk `item_node.parent()` one step; if it is `export_statement`, `Public`; otherwise `Private`. Class members use `accessibility_modifier` (public/protected/private) — out of scope for v1; class members default to `Public` if we cannot inspect the modifier, which matches the current `pub`-prefix behavior (always absent → always skipped in Rust-only logic, so zero regression).
- **Go**: visibility is encoded in identifier capitalization per the Go spec. First Unicode scalar of `display_name` is uppercase (via `char::is_ascii_uppercase` is sufficient for mainstream code; Go spec says Unicode Lu, but in practice ASCII-uppercase catches 99.9% of real Go code and the remainder is not worth the stdlib Unicode dep). → `Public`; otherwise `Private`.

### D3: Card filter includes `Public` and `Crate`, excludes `Private` and `Unknown`

Preserves Rust behavior:

- `pub fn foo` → `Public` → included (was: included).
- `pub(crate) fn foo` → `Crate` → included (was: included; signature starts with `pub`).
- `fn foo` → `Private` → excluded (was: excluded).

Non-Rust languages get semantically equivalent inclusion (`Public` for exported names).

Symbols carrying `Unknown` (pre-migration rows, or an extraction bug) are excluded. That is safer than over-including: the worst case is a brief card regression for un-touched files, which resolves on the next reconcile.

### D4: Tests

In `src/structure/parse/fixture_tests.rs` (existing table) — extend the `FIXTURES` entry type with an `expected_visibility: &[(&str, Visibility)]` field (keyed by symbol name) and assert it in the iterator that already walks every supported language.

New test in `src/surface/card/compiler/public_api.rs` tests module:

- `public_api_card_emits_for_python_non_dunder_names` — fixture with `class Public`, `class _Private`, `def __init__`, assert `public_symbols` contains `Public` and `__init__` but not `_Private`.
- `public_api_card_emits_for_typescript_export_decl` — fixture with `export class Foo` and `class Bar`, assert `Foo` is in, `Bar` is out.
- `public_api_card_emits_for_go_capitalized_ident` — fixture with `func Handle()` and `func helper()`, assert `Handle` is in, `helper` is out.

Regression guard in `src/structure/graph/tests` (or inline in `node.rs`): round-trip a `SymbolNode` JSON serialization with `visibility` absent; assert the result has `Visibility::Unknown`.

## Risks / Trade-offs

- **Go capitalization check is ASCII-only**: Go's language spec defines exported identifiers by Unicode category Lu, not ASCII-A-through-Z. A Go identifier starting with a non-ASCII uppercase letter (`Ölen`, `Üretici`) would be misclassified `Private`. Mitigation: use `c.is_uppercase()` instead of `c.is_ascii_uppercase()` at the cost of a small `char` stdlib call. Decide in implementation.

- **TypeScript class members**: `accessibility_modifier` (`public`/`protected`/`private`) on class members is not handled in v1. Members default to `Public`. This is a deliberate scoping call: class members are rarely called out as "the public API" of a directory in TS, and handling them properly requires walking into class bodies and tracking the `accessibility_modifier` child. The current `pub`-prefix logic skips them anyway, so defaulting to `Public` is a strict improvement.

- **`pub(crate)` semantics vs cross-language `Crate`**: Only Rust emits `Visibility::Crate` today. Python, TS, Go extractors never produce it. Card logic treats `Crate` as included, which matches Rust today. If a future extractor adds a `Crate`-like meaning (Java package-private?), revisit the card filter to make sure the output remains intuitive.

- **Storage compatibility window**: Older `.synrepo/` graphs read their symbols with `Visibility::Unknown` until reconcile rewrites them. Users who run `synrepo status` right after the upgrade may see a temporary drop in `synrepo_public_api` counts for Rust directories too. Mitigation: the compatibility report in `src/store/compatibility/` should flag this change as "advisory, reconcile refreshes visibility" — matching the pattern used for the `doc_comment` field addition. An explicit `synrepo reconcile` run closes the gap.

- **Serde `#[serde(default)]` on `Visibility` requires a `Default` impl**: `Visibility::Unknown` as `default` is the correct choice, but means the enum implements `Default`. This is fine today; callers that want an authoritative value can `match` on all four variants.

## Migration Plan

Single PR, no data migration. Behavior change:

1. `ExtractedSymbol` gains `visibility`; extractor updated for all four languages.
2. `SymbolNode` gains `visibility` with serde default `Unknown`.
3. Old JSON blobs in `symbols.data` deserialize as `Unknown`; they are excluded from `PublicAPICard` until reconcile rewrites them.
4. On next reconcile, every file is re-parsed and `visibility` is populated.
5. `synrepo_public_api` cards for non-Rust directories begin returning real entries.

Rollback is clean: removing the `visibility` field and reverting the card filter brings behavior back to Rust-only prefix matching. No data loss (the JSON blob is the serialized `SymbolNode`; the absent field just stops being read).

## Open Questions

- **O1**: Should class members in TypeScript get `accessibility_modifier`-based visibility in this change, or defer to v2? Current proposal: defer (always `Public` for now). Revisit if real-world TS corpora show the current behavior is noisy.
- **O2**: Should Python dunder names (`__init__`, `__enter__`) be `Public` or a new `Protocol` variant? Current proposal: `Public`, since they form part of the class's intended external contract. Mark as open for feedback during review.
- **O3**: Should the graph's compatibility report trip an advisory when it sees an older graph with no `visibility` populated, telling the user to reconcile? Current proposal: yes, as a soft advisory (matches the `doc_comment` precedent). Confirm during implementation.
