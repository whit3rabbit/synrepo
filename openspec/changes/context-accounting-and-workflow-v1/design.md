## Context

synrepo already returns graph-backed cards through CLI, MCP, and exports, with `tiny` / `normal` / `deep` tiers and advisory overlay labeling. The gap is that callers cannot consistently see the context budget contract, compare cards to raw-file reads, or measure whether agents are following the intended workflow.

## Goals / Non-Goals

**Goals:**
- Make context accounting visible on card-shaped responses without changing graph truth.
- Preserve existing tool names while adding clearer workflow aliases.
- Track context metrics in operational state, not in graph or overlay stores.
- Add a benchmark that measures card usefulness and token reduction together.
- Update agent-facing doctrine so the workflow is obvious.

**Non-Goals:**
- No generic agent memory, session memory, or chat-history storage.
- No LLM consolidation of source facts.
- No embeddings-first retrieval or embedding-defined source truth.
- No breaking removal of existing MCP tools or budget tier names.

## Decisions

1. **Use shared accounting metadata on cards.** Every card-shaped response carries `context_accounting` with token estimate, raw-file estimate, source hashes, stale state, and truncation state. Existing `approx_tokens` fields remain for compatibility until callers migrate.

2. **Keep budget tiers primary, add numeric caps as a hard ceiling where practical.** `tiny` / `normal` / `deep` still select field priority. Optional numeric caps limit card-set responses by dropping lower-ranked cards first, then marking truncation in accounting metadata.

3. **Store metrics under `.synrepo/state/`.** Context metrics are operational telemetry. They must be physically separate from graph and overlay stores and must never feed graph production or synthesis input.

4. **Add aliases, do not rename the product surface.** New CLI and MCP names (`orient`, `find`, `explain`, `impact`, `tests`, `changed`) wrap existing compiler behavior. Existing tool names stay supported.

5. **Benchmark against declared task fixtures.** The benchmark reads JSON task fixtures that declare query text and expected targets/tests. It reports reduction and hit rate so marketing claims remain evidence-backed.

## Risks / Trade-offs

- **Approximate token counts can be mistaken for exact billing numbers**: label them as estimates and use one estimator consistently.
- **Metrics writes from read paths can surprise users**: keep writes small, best-effort, and scoped to `.synrepo/state/`; failures must not break card retrieval.
- **Numeric caps can hide relevant context**: return `truncation_applied` and dropped-count metadata so agents know when to escalate.
- **Aliases can duplicate documentation**: keep one canonical doctrine block and reuse it in README, skill, and shim output.
