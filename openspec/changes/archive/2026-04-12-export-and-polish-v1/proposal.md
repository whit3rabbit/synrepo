## Why

Milestones 0–5 delivered the full structural and optional-intelligence core. Milestone 6 closes the adoption gap: generated exports let agents read structured summaries without a live MCP session, upgrade flows prevent stale `.synrepo/` state from poisoning updates, additional language coverage broadens repo compatibility, and onboarding polish reduces the friction between install and first useful output.

## What Changes

- **`synrepo export` command**: produce markdown or JSON views of card state, graph stats, and overlay findings so agents on repos without a running MCP server can read a snapshot; exports are labeled as convenience surfaces and are never synthesis input
- **Export freshness and repair**: generated export files participate in the repair loop as a new surface (`ExportSurface`); `synrepo check` reports stale exports, `synrepo sync` regenerates them
- **`synrepo upgrade` command**: detect `.synrepo/` version skew on binary update, run the appropriate compatibility action (continue / rebuild / migrate / block), and emit a clear upgrade report
- **Packaging polish**: verify `cargo install` and homebrew cask flows end-to-end; update cask and install docs
- **Additional language structural support**: add Go as a fully-supported structural language (tree-sitter grammar, symbol extraction, call/import queries); add a defined path for C/TypeScript JSX improvements
- **`synrepo agent-setup` improvements**: add `cursor`, `codex`, and `windsurf` tool targets; make the generated shim content match the current shipped MCP surface; add a `--regen` flag for in-place updates
- **`synrepo status` enrichment**: include last-upgrade info, export freshness summary, and overlay cost-to-date in the status output

## Capabilities

### New Capabilities
- none: all relevant areas already have durable specs

### Modified Capabilities
- `exports-and-views`: add concrete export command contract, supported format types, freshness derivation, and repair-loop surface requirements
- `bootstrap`: add upgrade flow requirements, `agent-setup` target improvements, and status output enrichment
- `substrate`: add Go structural support as a concrete language target; define when to add vs. defer additional languages
- `storage-and-compatibility`: add `synrepo upgrade` command contract with defined compatibility actions and CLI output contract
- `repair-loop`: add `ExportSurface` as a new repair surface with `RegenerateExports` action alongside the existing commentary and cross-link surfaces

## Impact

- `src/bin/cli.rs` and `src/bin/cli_support/commands/`: new `export` and `upgrade` subcommands
- `src/pipeline/repair/types/stable.rs`: new `RepairSurface::ExportSurface` and `RepairAction::RegenerateExports` variants (dual-mapping update required)
- `src/pipeline/repair/report.rs` and `sync.rs`: export surface detection and regeneration
- `src/structure/parse/` and `Cargo.toml`: `tree-sitter-go` grammar dependency; new Go adapter
- `src/surface/card/compiler/` and `src/bin/cli_support/commands/`: export rendering from cards to markdown/JSON
- `src/store/compatibility/`: upgrade-on-startup path; version-skew detection
- `crates/synrepo-mcp/src/main.rs`: verify tool description strings match current surface (no new tools)
- `packaging/homebrew/` and `AGENTS.md`: packaging artifact updates
