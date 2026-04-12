## 1. Durable spec: overlay (commentary only)

- [x] 1.1 Apply the `overlay` delta spec to `openspec/specs/overlay/spec.md`: update purpose line to commentary-only scope, replace all five requirements with the commentary-specific versions from the delta (overlay-only content, minimum provenance fields, freshness states, retrieval boundaries, audit exposure, cost and generation controls), remove the bounded-evidence-verified-linking requirement
- [x] 1.2 Confirm `openspec/specs/overlay/spec.md` no longer contains any requirement that references cross-link candidate generation, verification, confidence scoring, or review surfaces

## 2. Durable spec: overlay-links (new)

- [x] 2.1 Create `openspec/specs/overlay-links/spec.md` from the delta spec: purpose line, and four requirements (evidence-verified candidates, confidence scoring, review and promotion workflow, audit trail)
- [x] 2.2 Confirm `openspec/specs/overlay-links/` directory and `spec.md` file exist and all four requirements have at least one `#### Scenario:` each

## 3. Durable spec: mcp-surface

- [x] 3.1 Apply the `mcp-surface` delta spec to `openspec/specs/mcp-surface/spec.md`: update the "Require provenance and freshness in responses" requirement to include the four commentary state definitions (present-and-fresh, present-and-stale, absent, budget-withheld) and the two new scenarios (stale commentary, budget-withheld commentary)
- [x] 3.2 Confirm the two existing scenarios are preserved unchanged and the two new scenarios are correctly added

## 4. Durable spec: repair-loop

- [x] 4.1 Apply the `repair-loop` delta spec to `openspec/specs/repair-loop/spec.md`: update the "Define targeted drift classes" requirement to rename "stale overlay entries" to "stale commentary overlay entries" and add the cross-link-surface-as-unsupported scenario
- [x] 4.2 Confirm the three existing scenarios are present and the new "Report cross-link review surface as unsupported" scenario is added

## 5. ROADMAP.md updates

- [x] 5.1 Update section 7.12 in `ROADMAP.md`: split the single `openspec/specs/overlay/spec.md` entry into two entries — Track J governing `overlay/spec.md` (commentary) and Track K governing `overlay-links/spec.md` (cross-links). Remove the current single "Ties to roadmap: Track J, Track K" coupling.
- [x] 5.2 Update section 8.11 commentary in `ROADMAP.md` to reflect that `commentary-overlay-v1` is scoped to commentary-only and that cross-link overlay work is deferred to `cross-link-overlay-v1` (section 8.12)
- [x] 5.3 Confirm `ROADMAP.md` section 8.12 (`cross-link-overlay-v1`) references `openspec/specs/overlay-links/spec.md` as its governing spec

## 6. Validation

- [x] 6.1 Run `openspec status --change "commentary-overlay-v1"` and confirm all artifacts show `done`
- [x] 6.2 Confirm `openspec/specs/overlay/spec.md` has no remaining cross-link requirements (`grep -i "cross-link\|proposed link\|candidate\|confidence" openspec/specs/overlay/spec.md` returns no hits under a requirement heading)
- [x] 6.3 Confirm `openspec/specs/overlay-links/spec.md` exists and contains all four requirements
- [x] 6.4 Run `cargo build` and confirm no compilation errors (spec-only change; no Rust files modified, but confirm the existing `src/overlay/mod.rs` boundary is undisturbed)
