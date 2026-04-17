# AGENTS.md

> `CLAUDE.md` is a symlink to `AGENTS.md`. Edit `AGENTS.md`; both update.

Git intelligence pipeline: symbol revision tracking, co-change analysis, ownership attribution.

## Key files

- `mod.rs` — facade, type exports
- `types.rs` — public payloads (`GitFileSummary`, `CoChangesWith`, etc.)
- `analysis.rs` — history/hotspot/ownership derivation
- `emit.rs` — `CoChangesWith` edge emission
- `index.rs` — per-symbol first/last seen rev tracking
- `symbol_revisions/` — body-hash diffing for symbol history

## Hard invariants

- Uses `gix` (not git2), rewrite tracking disabled for perf
- `CoChangesWith` edges use `Epistemic::GitObserved`
- Symbol first/last seen rev tracked via body-hash diffing
- History depth bounded by `config.git_commit_depth` (default 500)
