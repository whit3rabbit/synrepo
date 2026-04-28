# Design: Symbol-to-symbol Calls edges (Stage 5 resolver widening)

## Decision 1 — Widen `EdgeKind::Calls` vs. introduce a new edge kind

Widen `EdgeKind::Calls` to allow symbol endpoints on either side. Rationale:

- The drift-scoring pipeline, graph compaction, and edge retirement flow are all indifferent to whether the endpoint is a file or a symbol — they key on `(from, to, kind)` tuples. Widening costs nothing in those layers.
- A new edge kind would force every consumer to dual-read. That's a bigger blast radius than widening.
- The `EdgeKind::Calls` docstring gains one sentence noting the widened semantics. The type system already allows both endpoint shapes (`NodeId` is an enum).

File-scoped `Calls` edges remain valid during the transition window; stage 4 dual-emits file-to-symbol and symbol-to-symbol calls for call sites with an enclosing caller. Parser-owned stage-4 edges now carry `owner_file_id`, so obsolete call/import rows retire on the next content-changing compile when they are not re-emitted. `compact_retired_observations` cleans up after `retain_retired_revisions` cycles.

Consumer audit:

- Drift scoring and compaction operate on generic edge IDs and active/retired edge rows, so widening `Calls` endpoints does not require a new edge kind.
- Call-path, neighborhood, entry-point, test-surface, MCP search, and symbol-card compilers already query `EdgeKind::Calls` through `NodeId` endpoints. Consumers that need symbol-specific behavior must filter endpoints to `NodeId::Symbol`.
- Existing file-card and file-scoped call tests remain valid during the transition because stage 4 dual-emits file-to-symbol and symbol-to-symbol call edges for calls with an enclosing caller.

## Decision 2 — Body-scope resolution rubric for ambiguous overloads

The resolver walks each call ref under a specific caller symbol. For each callee name it scores candidates:

1. **In-file symbol matching the name** — highest priority (scope 0 = local).
2. **Symbol in an imported file matching the name** — scope 1. When multiple imported files expose the same name, apply the existing cross-file scoring rules (visibility, graph distance, path-segment match).
3. **Receiver-qualified method call (Python `self.method`, TS `obj.method`)** — if the receiver has a resolvable type in the local scope (literal type annotation, constructor call in the same function, `self` inside a class), resolve against that type's symbol set.
4. **Dynamic-dispatch receivers with no inferable type** — drop the edge silently (same policy as the existing "ambiguous call name no imports is dropped" rule at `stage4_ambiguous_call_name_no_imports_is_dropped`).

Python inheritance and duck typing are not handled in this iteration — they remain a Phase-2+ boundary and should be called out in `docs/FOUNDATION.md`. When a `self.method` call could target inherited or monkey-patched behavior, the resolver only emits an edge if the local symbol set has a positive-scoring concrete candidate; otherwise it drops the edge. TypeScript class-method dispatch follows the same rule: direct class-name or locally visible method candidates can score, but unresolved structural/interface dispatch is not inferred.

## Decision 3 — Caller-symbol identification

Extend `ExtractedCallRef` with the caller symbol's `(qualified_name, body_hash)` pair. During parse-time extraction, the extractor walks up from the call-expression node until it finds an enclosing `function_item` / `function_definition` / `method_definition` / `function_declaration` node. If it reaches the root without finding one, the call is at module scope and the existing file-scoped edge is emitted as a fallback (no caller symbol exists for module-scope statements).

## Decision 4 — Migration path

- First compile after this code lands: emits BOTH file-scoped and symbol-scoped Calls edges for every successfully resolved call site that has an enclosing caller. Module-scope call refs keep only the file-scoped edge.
- Rollback-safe: the widened edge kind is backward-compatible with older readers. A reader that expects only file-scoped Calls will still see file-scoped rows (during the transition window) and can ignore symbol-scoped rows.
- `compact_retired(older_than_rev)` runs during `synrepo sync` and `synrepo upgrade --apply` as today; no new command needed.

## Decision 5 — Card surface

`SymbolCard.callers` and `callees` at Deep budget populate from:

- `graph.outbound(NodeId::Symbol(id), Some(EdgeKind::Calls))` for callees.
- `graph.inbound(NodeId::Symbol(id), Some(EdgeKind::Calls))` for callers.

Normal and Tiny budgets keep empty arrays (matching the current withholding policy for other Deep-only fields).

The MCP `synrepo_call_path` tool speced at `openspec/specs/mcp-surface/spec.md:281` starts to return nonempty results naturally once this lands.

## Open questions

1. Do we emit symbol-scoped Calls for calls from module-top-level statements (no enclosing function)? Proposed: **no** — those keep the existing file-scoped edge. Reader can distinguish via `from.kind()`.
2. Cross-language calls (e.g., TS calling into a WASM-bound Rust symbol) — out of scope here. Continue to drop silently.
3. Built-in / standard-library calls (`println!`, `print(...)`) — continue to drop silently (no target symbol in the graph).
