## Context

Three follow-ups from the review extend the operator surface without changing the graph or overlay contracts. Each keeps existing flows unchanged and adds narrow, optional affordances:

- `synrepo doctor` aggregates the existing status snapshot into a degradation-only view suitable for CI.
- Prometheus text export turns the already-tracked context metrics into scrapeable output; an HTTP endpoint behind a cargo feature lets operators poll the same text over the wire.
- Multi-client setup (`--only` / `--skip`) replaces per-tool loops around the CLI.

## Goals / Non-Goals

**Goals:**

- Land each subcommand and flag as an additive extension that does not alter existing invocations.
- Preserve a single source of truth: `synrepo doctor` is a view over `StatusSnapshot`, not a second scan.
- Prometheus output shares a single formatter between stdout and the (feature-gated) HTTP endpoint.

**Non-Goals:**

- No new metric fields on `ContextMetrics`.
- No auth on the HTTP endpoint in v1; localhost-bind default, documented as optional-and-unauthenticated.
- No shared "server" subcommand for MCP and metrics; MCP remains stdio and the metrics HTTP server is its own small binary behind `metrics-http`.
- No introspection beyond context metrics (no graph stats, reconcile timings, etc.) in v1.

## Decisions

1. **`synrepo doctor` is a filter over `StatusSnapshot`.** Reusing the existing snapshot builder guarantees the doctor view never diverges from `synrepo status`. The filter pulls the same rows `build_health_vm` emits and keeps only those whose severity is not `Healthy`.

2. **Doctor exits non-zero on degradation.** An operator running `synrepo doctor` in a pre-commit hook or CI wants a process-level failure, not a string search. The text output is secondary.

3. **One Prometheus formatter, two surfaces.** `ContextMetrics::to_prometheus_text` is the single point of change for metric names. `stats context --format prometheus` and the HTTP endpoint both call it. This mirrors the status-snapshot pattern.

4. **`metrics-http` is off by default.** The HTTP server is dependency weight that most operators do not want. The feature flag keeps the default build lean. A CI matrix entry compiles with the feature enabled to prevent rot.

5. **`--only` / `--skip` are mutually exclusive.** Mixing them produces unreadable precedence rules. A clear error at parse time is better than silent surprise.

6. **Positional single-tool invocations are preserved.** The existing happy path (`synrepo setup claude`) continues to work unchanged. The new flags kick in only when `--only` or `--skip` is provided.

7. **Unknown tools in `--only` / `--skip` hard-fail.** Spelling mistakes should be caught early. The detection layer already enumerates supported tools.

## Risks / Trade-offs

- The `metrics-http` feature introduces an HTTP stack (likely `tiny_http`) that must be reviewed and approved. Until the dep is confirmed, the HTTP endpoint remains in plan form.
- A degraded `StatusSnapshot` may be missing fields (e.g. commentary coverage unavailable). The doctor view must treat "unknown" as Stale, not Blocked, to avoid false alarms when the overlay is unreadable but the graph is fine.
- Prometheus metric naming is stable-by-contract. Renames after release are breaking. The initial set is intentionally small (the five counters above) so the contract stays narrow.

## Out-of-scope follow-ups

- A metrics exposition for explain accounting (commentary cost, per-provider usage). That surface already exists as JSON under `.synrepo/state/explain-log.jsonl` and could be added in a later change.
- Auth for the HTTP endpoint. Localhost bind is the only supported deployment in v1.
