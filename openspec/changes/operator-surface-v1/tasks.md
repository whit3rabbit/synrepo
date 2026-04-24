## 1. `synrepo doctor` aggregation command

- [x] 1.1 Add a `Doctor { json: bool }` variant to the CLI args (current location: `src/bin/cli_support/cli_args/`).
- [x] 1.2 Add `src/bin/cli_support/commands/doctor.rs` that reads `status_snapshot::build_status_snapshot` and prints only components whose severity is not `Healthy`; supports `--json`.
- [x] 1.3 Exit non-zero when any degraded component is found.
- [x] 1.4 Unit tests covering healthy snapshot → exit 0 and degraded snapshot → exit 1.
- [x] 1.5 Dispatch from `src/bin/cli.rs`.

## 2. Prometheus text format for context metrics

- [x] 2.1 Add `ContextMetrics::to_prometheus_text(&self) -> String` in `src/pipeline/context_metrics.rs`.
- [x] 2.2 Emit `synrepo_cards_served_total`, `synrepo_card_tokens_total`, `synrepo_raw_file_tokens_total`, `synrepo_estimated_tokens_saved_total`, `synrepo_stale_responses_total`, and escalation/truncation counters.
- [x] 2.3 Add a golden-string unit test against a well-known `ContextMetrics` snapshot.
- [x] 2.4 Extend `stats context` with `--format text|json|prometheus`; default stays `text`.

## 3. `--only` / `--skip` multi-client setup

- [x] 3.1 Add `--only <tool,tool>` and `--skip <tool,tool>` to `Setup` and `AgentSetup` CLI args. Both flags mutually exclusive.
- [x] 3.2 Resolver unit tests: only / skip / both-set error / unknown-tool error / skip-missing policy.
- [x] 3.3 Update `src/bin/cli_support/commands/setup/mod.rs` (and/or the agent-setup dispatch) to iterate the resolved list and report per-tool outcome.
- [x] 3.4 When no flags and a positional tool are given, preserve existing single-tool behavior.

## 4. HTTP `/metrics` endpoint (feature-gated)

- [ ] 4.1 Add cargo feature `metrics-http` (default off). Choose the HTTP dep (prefer `tiny_http`; confirm with user before adding a heavier stack).
- [ ] 4.2 Add `src/bin/cli_support/commands/server.rs` behind `#[cfg(feature = "metrics-http")]` with `synrepo server --metrics <addr>`.
- [ ] 4.3 Integration test that binds `127.0.0.1:0`, requests `/metrics`, and asserts Prometheus parseability.
- [ ] 4.4 Add a CI job that builds `cargo build --features metrics-http` so the gated path does not rot.

## 5. Verification

- [x] 5.1 `cargo test` covering `commands::doctor`, `context_metrics::to_prometheus_text`, and the setup resolver. (Feature-gated server remains task #4.)
- [ ] 5.2 `cargo run -- doctor` returns 0 on a healthy repo; returns non-zero on a stale repo.
- [x] 5.3 `cargo run -- stats context --format prometheus` emits scrapeable text.
- [ ] 5.4 `cargo run --features metrics-http -- server --metrics 127.0.0.1:9090` serves `GET /metrics`.
- [x] 5.5 `cargo run -- setup --only claude,cursor` wires both; `--only claude --skip claude` errors (clap-level conflict).
- [ ] 5.6 `make check` passes.
- [ ] 5.7 `openspec validate operator-surface-v1` and `openspec status --change operator-surface-v1 --json` shows `isComplete: true`.
