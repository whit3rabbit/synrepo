## Context

Commentary generation has two prompt assembly paths. Repair sync builds a richer context from graph facts, source snippets, related files, tests, and module peers. Explicit card refresh builds context from a deep `SymbolCard`, which is narrower and only supports symbols. Both paths feed the same provider boundary and the same overlay store, so divergent context quality is unnecessary and risks inconsistent commentary.

The graph is the canonical source for parser-observed code facts. Overlay commentary, proposed links, and materialized explain docs are advisory outputs and must not feed explain generation.

## Goals / Non-Goals

**Goals:**
- Provide one shared explain context builder for explicit refresh and repair sync.
- Include direct graph neighborhood facts that help explain connected code.
- Keep `commentary_cost_limit` as the only public input budget control.
- Preserve graph/overlay separation and read snapshots for multi-query graph reads.

**Non-Goals:**
- No CLI, MCP, provider, or config schema changes.
- No raw AST dump in prompts.
- No graph degree greater than one in v1.
- No use of overlay commentary, proposed links, or explain docs as prompt input.

## Decisions

- Use graph-backed context instead of raw AST. The graph already stores the useful AST-derived facts in a compact, queryable form, while raw AST would add prompt noise and language-specific formatting.
- Add `src/pipeline/explain/context/` as the shared owner. This keeps prompt construction with the explain pipeline and lets repair sync become a caller rather than the owner of explain prompt shape.
- Rank prompt blocks by utility. Target identity, signature, doc comment, visibility, and source snippet are core; imports/imported-by, calls, visible exports, tests, governing decisions, co-change partners, related source snippets, and module peers are optional in that order.
- Enforce budget by construction. The builder estimates tokens with the provider-compatible context estimator and drops optional blocks before truncating source snippets. Providers still perform their own final budget check.
- Treat exports as visible public surface. V1 uses `SymbolNode.visibility` (`public` or `crate`) plus `SymbolKind::Export`; it does not add a new edge type.

## Risks / Trade-offs

- Richer prompts can exceed conservative default budgets, mitigation: deterministic trimming keeps the final prompt within `commentary_cost_limit` whenever possible.
- File and symbol refresh paths may need slightly different target metadata, mitigation: resolve both through `CommentaryNodeSnapshot`.
- Degree-one graph context may miss transitive behavior, mitigation: keep v1 bounded and add degree-two only after evidence shows it is needed.
- Moving repair prompt assembly risks regressions, mitigation: leave a repair wrapper and add tests proving both entrypoints share the new builder.
