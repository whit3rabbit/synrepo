# Runtime Budget

This document is a review guardrail for runtime and storage growth. It does not
define new runtime behavior. Budgets here are hard caps only when the product
already enforces a cap; otherwise they are review expectations that must be
measured and justified when they change.

Use this document when a change touches graph snapshots, context-serving
metrics, overlay growth, embedding storage, LLM response cache storage, stale
advisory content, or card-serving latency.

## Collect Metrics

Use existing surfaces before adding new instrumentation:

```bash
synrepo status --json
synrepo status --recent
synrepo bench context
du -sh .synrepo/overlay .synrepo/index/vectors .synrepo/cache/llm-responses
```

`synrepo bench context` is required for context-savings claims. Token reduction
alone is not enough; include target hit rate, miss rate, stale rate, and latency.

## Graph Snapshot Budget

`Config.max_graph_snapshot_bytes` defaults to `128 MiB`. This is advisory:
oversized snapshots may still publish with a warning, and `0` disables snapshot
publication.

Review expectations:

- Graph-only MCP card response latency should stay below `50ms p99`.
- Idle memory for a 10k-file repository should stay below `500 MB RSS`.
- Any increase to the graph snapshot default must explain why the existing
  SQLite fallback or card budget surface is insufficient.
- If snapshot publication is disabled or bypassed, card latency before and
  after the change must be reported.

## Context Metrics Budget

`ContextMetrics` separates observed counters from estimated counters. Preserve
that distinction in status output, docs, telemetry, and any future exports.

Observed counters include:

- `cards_served_total`
- `budget_tier_usage`
- `truncation_applied_total`
- `stale_responses_total`
- `test_surface_hits_total`
- `changed_files_total`
- `context_query_latency_ms_total`
- `context_query_latency_samples`
- `mcp_requests_total`
- `mcp_tool_calls_total`
- `mcp_tool_errors_total`
- `mcp_tool_error_codes_total`
- `saved_context_writes_total`
- `compact_outputs_total`
- `compact_omitted_items_total`

Estimated counters include card token totals, raw-file comparison totals,
estimated token savings, compact returned/original token totals, and compact
estimated token savings.

Review expectations:

- Do not present estimated token savings as proof that an external agent avoided
  file reads.
- If `stale_responses_total` rises materially, explain whether the change is
  surfacing more stale advisory content or making existing advisory content
  stale more often.
- If `context_query_latency_ms_avg` rises materially, include before and after
  measurements from `synrepo status --json` or benchmark output.
- New context metrics must avoid storing prompts, queries, claims, caller
  identity, response bodies, or note text.

## Overlay Growth Budget

The overlay store holds advisory content only: commentary, cross-link
candidates, findings, and explicit agent notes. Overlay growth must not affect
canonical graph truth, and stale advisory content must remain labeled.

Review expectations:

- Commentary growth should be tied to explicit refresh or repair actions, not
  silent read-path writes.
- Cross-link candidates should stay bounded by review queues, confidence tiers,
  and store-side limits.
- Agent notes must stay explicit saved-context actions, not automatic session
  memory.
- Any change that increases stale advisory responses must explain how callers
  can distinguish fresh, stale, missing, invalid, and budget-withheld content.
- Any change that adds overlay rows must preserve provenance, source store
  labels, advisory labels, and drift/freshness behavior.

## Embedding And LLM Cache Budget

The retention targets from the foundation design are:

| Store | Budget | Review action |
| --- | ---: | --- |
| `.synrepo/index/vectors/` | `2 GB` | Explain the semantic-triage need and verify rebuild or eviction behavior. |
| `.synrepo/cache/llm-responses/` | `1 GB` | Explain why cached LLM responses are needed and verify pruning behavior. |

Semantic triage remains opt-in. The embedding model default is
`all-MiniLM-L6-v2` with `384` dimensions. A model or dimension change must
explain rebuild impact and compatibility behavior.

## Stale Response And Latency Guardrails

Graph-backed card fields should be fresh after reconcile. Overlay-backed fields
may be fresh, stale, missing, unsupported, invalid, or budget-withheld, and those
states must remain visible to callers.

Review expectations:

- Graph-backed card reads should remain fast and should not wait for LLM work.
- Commentary refresh remains explicit and should not silently spend provider
  budget on ordinary reads.
- Stale advisory content is acceptable only when labeled.
- Any intentional card latency regression must include measured before and
  after values plus the reason the extra work belongs on the read path.
