## Why

Stage 4 resolves call sites from the importing file to the callee's **file**, not to the callee's **symbol**. As a direct consequence, `SymbolCard.callers` and `SymbolCard.callees` are hardcoded to empty arrays (`src/surface/card/compiler/symbol.rs:42-45`):

```rust
// Phase 1: edges are file->symbol, not symbol->symbol.
// Empty until symbol->symbol Calls edges land in stage 5.
let callers: Vec<SymbolRef> = vec![];
let callees: Vec<SymbolRef> = vec![];
```

`docs/FOUNDATION.md:121` documents this as a Phase-1 boundary: *"callers/callees: shipped (file-scoped Calls edges; symbol-to-symbol is approximate)"*. The MCP spec at `openspec/specs/mcp-surface/spec.md:281` already defines `synrepo_call_path(target, budget?)` as *"backward BFS over `Calls` edges with depth budget"*, so the surface is speced but the underlying data is not yet there.

The gap surfaced in an audit of deferred work in the codebase and is queued for a dedicated change rather than bundling it into unrelated repair work.

## What Changes

- Resolve each call site to a `NodeId::Symbol(callee)` by matching the callee name against the resolved file's symbol set. Current resolution stops at file.
- Carry the caller symbol through stage-4 so emitted edges use `from: NodeId::Symbol(caller)` instead of `from: NodeId::File(importer)`.
- Decide widen-vs-replace: keep `EdgeKind::Calls` and allow both file-scoped and symbol-scoped edges (differentiated by `from.kind()`), OR replace file-scoped Calls with symbol-scoped. Recommended: **widen**, to preserve drift-scoring and retirement behavior for the transition window.
- Populate `SymbolCard.callers`/`callees` by walking `Calls` edges whose endpoints are symbols.
- Thread caller symbol through `ExtractedCallRef` so the resolver sees `(caller, callee_name)` pairs rather than `(file, callee_name)`.

## Capabilities

### New Capabilities

None. Refines an existing capability.

### Modified Capabilities

- `graph-semantics`: `EdgeKind::Calls` widens to allow symbol endpoints on both sides. Graph-store retirement and compaction behavior apply unchanged.
- `cards`: `SymbolCard.callers` and `callees` populate from the new edges.

## Impact

- `src/structure/parse/extract/mod.rs::extract_call_refs` — thread caller symbol through `ExtractedCallRef`.
- `src/pipeline/structural/stage4/mod.rs` — body-scope resolver: per-symbol walk of call refs, lookup in in-file scope first, then imported-file scope.
- `src/pipeline/structural/stage4/scoring.rs` — scoring rubric for ambiguous overloads (Python `self.method`, TS class method dispatch).
- `src/pipeline/structural/tests/edges/{rust,python,typescript,go}.rs` — per-language symbol-to-symbol edge tests.
- `src/surface/card/compiler/symbol.rs` — populate `callers`/`callees` from the new edges.
- `src/structure/graph/edge.rs` — documentation update for the widened Calls semantics.
- Migration plan: existing file-scoped Calls edges retire on first compile after the new code lands; new symbol-scoped Calls edges populate alongside. `compact_retired` cleans up retired rows after `retain_retired_revisions` cycles.
- Identity model check: when a caller symbol's `body_hash` changes, its owning `(caller, callee)` edges invalidate via the existing retirement flow. Verify.
