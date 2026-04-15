## Types

### PublicAPIEntry

One exported symbol in a `PublicAPICard`.

```rust
pub struct PublicAPIEntry {
    pub id: SymbolNodeId,
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,        // full declaration prefix; visibility readable from it
    pub location: String,         // "path:byte_offset" for navigation
    pub last_change: Option<SymbolLastChange>,  // absent at Tiny
}
```

### PublicAPICard

```rust
pub struct PublicAPICard {
    pub path: String,
    pub public_symbols: Vec<PublicAPIEntry>,      // empty at Tiny
    pub public_symbol_count: usize,               // always present
    pub public_entry_points: Vec<PublicAPIEntry>, // empty at Tiny
    pub recent_api_changes: Vec<PublicAPIEntry>,  // Deep only; skipped when empty
    pub approx_tokens: usize,
    pub source_store: SourceStore,
}
```

## Budget Gating

| Field | Tiny | Normal | Deep |
|---|---|---|---|
| `path`, `public_symbol_count`, `approx_tokens`, `source_store` | yes | yes | yes |
| `public_symbols`, `public_entry_points` | empty | populated | populated |
| `PublicAPIEntry.last_change` | absent | present (no summary) | present (with summary) |
| `recent_api_changes` | empty | empty | populated (30-day window) |

## Visibility Inference

`SymbolNode.signature` contains the full declaration up to `{` or `;`, e.g. `pub fn parse(...)` or `pub(crate) struct Foo`. A symbol is public if:

```rust
symbol.signature.as_deref().map_or(false, |s| s.starts_with("pub"))
```

This catches `pub`, `pub(crate)`, `pub(super)`, `pub(in path)`. All are included in v1 since the full signature is present in `PublicAPIEntry.signature` and callers can discriminate.

**Limitation:** This heuristic is Rust-specific. TypeScript uses `export`, Python has no visibility keyword, Go uses capitalization. For non-Rust files, `public_symbols` will be empty. A dedicated `visibility` field on `SymbolNode` is the right long-term fix; deferred.

## Entry Point Detection

Reuse `entry_point::classify_kind(qname, path, kind)` from `src/surface/card/compiler/entry_point/mod.rs`. Change visibility from `fn` to `pub(super)` so `public_api.rs` can call it. Public entry points are the intersection of public symbols and symbols for which `classify_kind` returns `Some(_)`.

## Recency Window

`RECENT_API_DAYS: i64 = 30`. A symbol qualifies for `recent_api_changes` if:

```rust
last_change.committed_at_unix > now_unix() - RECENT_API_DAYS * 86400
```

`now_unix()` uses `std::time::SystemTime::now().duration_since(UNIX_EPOCH)`. When `last_change` is `None` (no git context), the symbol is excluded from `recent_api_changes`.

## Token Estimate

```
approx_tokens = public_symbol_count * per_symbol + 20
```

Where `per_symbol` is 10 (Tiny, symbols not materialised), 30 (Normal), 60 (Deep).

## Graph Access Pattern

Follows `module_card_impl` exactly:
1. `graph.all_file_paths()` → filter to direct children of `path/`
2. For each file: `graph.outbound(NodeId::File(id), Some(EdgeKind::Defines))` → symbol IDs
3. `graph.get_symbol(sym_id)` → full `SymbolNode`
4. Filter on `signature.starts_with("pub")`
5. For Normal/Deep: `compiler.resolve_file_git_intelligence(&file_path)` → `symbol_last_change_from_insights`

## MCP Tool

```
synrepo_public_api(path: str, budget?: str) -> PublicAPICard (JSON)
```

- `path`: directory path, e.g. `"src/auth"` or `"src/surface/card"`
- `budget`: `"tiny"` (default) | `"normal"` | `"deep"`
- Follows the same `with_graph_snapshot` + `render_result` pattern as `synrepo_module_card`
