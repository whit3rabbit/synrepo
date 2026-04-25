# AGENTS.md

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

Repair pipeline: drift detection, surfaces, and auto-repair actions.

## Key files

- `mod.rs` — facade, exports
- `types.rs` — re-exports stable enums from `types/stable.rs` (`RepairSurface`, `DriftClass`, `Severity`, `RepairAction`) and payloads from `types/models.rs`
- `sync/` — auto-repair orchestration: `mod.rs`, `handlers.rs` (dispatch match, already over the 400-line cap — land new handlers in sibling files), `commentary.rs`, `revalidate_links.rs`, plus `commentary_plan/` sub-module
- `report/` — drift report builder; `surfaces/` has 11 `SurfaceCheck` implementations across 6 files: `mod.rs`, `agent_notes.rs`, `commentary.rs`, `cross_links.rs`, `drift.rs`, `rationale.rs`
- `log.rs` — JSONL resolution log append
- `commentary.rs` — commentary refresh repair action
- `cross_links.rs` — cross-link generation pass
- `cross_link_verify/` — validates existing cross-link overlay rows (`io.rs`, `matching.rs`, `mod.rs`)
- `declared_links.rs` — verifies human-declared `Governs` targets

## Hard invariants

- `repair/types/stable.rs` has dual string mappings (serde + `as_str()`) — must stay in sync
- Repair actions run via `synrepo sync`
- `RepairSurface::ProposedLinksOverlay` and `RepairSurface::ExportSurface` exist
- Commentary freshness scanned in two places: the status command (`src/bin/cli_support/commands/status/text.rs` and `status/json.rs`) and the repair surface (`scan_commentary_staleness` in `report/surfaces/commentary.rs`)
