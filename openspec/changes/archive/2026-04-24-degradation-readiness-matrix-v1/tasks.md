## 1. Readiness Model

- [x] 1.1 Define shared readiness labels and severity mapping.
- [x] 1.2 Map parser, git, embeddings, watch, index freshness, overlay, and compatibility states into the matrix.
- [x] 1.3 Add next-action fields for each matrix row.

## 2. Surfacing

- [x] 2.1 Extend runtime probe or status snapshot output with capability readiness entries.
- [x] 2.2 Update status, doctor, and dashboard renderers to consume the shared matrix.
- [x] 2.3 Update bootstrap success or partial output to report degraded capabilities.

## 3. Behavior Tests

- [x] 3.1 Add tests for parser failure, no git, stale index, overlay unavailable, watch disabled, and compat-blocked rows.
- [x] 3.2 Verify optional disabled features do not block core graph-backed operation.
- [x] 3.3 Verify degraded card or workflow output labels unavailable data sources.

## 4. Verification

- [x] 4.1 Run focused runtime-probe/status/dashboard tests.
- [x] 4.2 Run `cargo test` for affected diagnostics.
- [x] 4.3 Run `openspec validate degradation-readiness-matrix-v1`.
- [x] 4.4 Run `openspec status --change degradation-readiness-matrix-v1 --json` and confirm `isComplete: true`.
