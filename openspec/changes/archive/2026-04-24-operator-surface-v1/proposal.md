## Why

After the context-accounting + workflow-alias work landed, three operator-surface gaps remained from the original review:

1. Health signals are useful, but there is no one-line **aggregator** an operator can run in CI or in a terminal ("is anything wrong?"). `synrepo status` prints a full report; `synrepo check` is drift-specific.
2. Context metrics are captured at `.synrepo/state/context-metrics.json`, but the only way to scrape them is to parse JSON. Operators running Prometheus-flavored monitoring have no first-class text export.
3. `synrepo setup <tool>` and `synrepo agent-setup <tool>` accept exactly one tool. Teams that want to wire up Claude, Cursor, and Codex in one pass have to script three sequential invocations.

These are narrow, high-value additions that do not touch the graph, overlay, or explain pipelines. They extend the operator surface only.

## What Changes

- **`synrepo doctor` command.** A narrow view over the existing status snapshot that surfaces only components whose severity is not `Healthy`. Exits zero when all components are healthy, non-zero when any are stale or blocked. Reuses `status_snapshot::build_status_snapshot` verbatim so there is no second snapshot code path.
- **Prometheus text format for context metrics.** `ContextMetrics::to_prometheus_text` emits the existing counters in Prometheus exposition format. `synrepo stats context --format prometheus` routes to it. No new metric fields.
- **Optional HTTP `/metrics` endpoint behind a cargo feature.** A new `synrepo server --metrics <addr>` subcommand, gated by a `metrics-http` cargo feature (off by default), binds to an address and serves `GET /metrics` with the Prometheus text body. Implementation detail: single-threaded local HTTP, localhost-bind default, no auth. The feature must compile in CI so it does not silently rot.
- **Multi-client `--only` / `--skip` flags for setup.** `synrepo setup` and `synrepo agent-setup` gain `--only <tool,tool>` and `--skip <tool,tool>` arguments. When both flags are unset and a positional `<tool>` is given, existing behavior is preserved. When `--only` or `--skip` is set, the command iterates over the resolved tool list and reports per-tool outcome. Mutually exclusive with `--only` and `--skip` supplied together.

## Capabilities

- **Modified:** `context-accounting` — adds the Prometheus exposition format to the metrics-inspection requirement.
- **Modified:** `dashboard` — adds the `synrepo doctor` aggregation view.
- **Modified:** `bootstrap` — adds the `--only` / `--skip` flags to the agent-setup target expansion.
- **Added:** `operator-surface` — the optional HTTP metrics endpoint (new capability, feature-gated).

## Impact

- No new graph or overlay semantics. No `Epistemic` variants, no new edges, no explain pipeline changes.
- New optional HTTP dependency gated behind a cargo feature; the dep choice (`tiny_http` vs. reusing an MCP-side HTTP lib) is flagged for confirmation at implementation time.
- CI must add a `--features metrics-http` build so the gated code path does not rot.
- `synrepo doctor` is additive; existing `synrepo status` behavior is unchanged.
- `--only` / `--skip` are additive flags; single-positional-tool invocations continue to work unchanged.
