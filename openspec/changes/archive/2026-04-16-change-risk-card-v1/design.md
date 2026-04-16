# change-risk-card-v1 Design

## Context

The graph store contains:
- Drift scores via `edge_drift` table (Jaccard distance on structural fingerprints)
- Co-change edges via `CoChangesWith` edges from git intelligence
- Git hotspot data via the `GitHotspotSummary` in git-intelligence payloads

The card system (`src/surface/card/`) produces cards via GraphCardCompiler. Existing card types: SymbolCard, FileCard, ModuleCard, EntryPointCard, PublicAPICard, DecisionCard, MinimumContextCard, CallPathCard, TestSurfaceCard.

## Goals / Non-Goals

**Goals:**
- Create ChangeRiskCard aggregating drift, co-change, and hotspot signals
- Implement synrepo_change_risk MCP tool
- Wire CLI command `synrepo change-risk <target>`

**Non-Goals:**
- Machine learning or predictive risk modeling — derive score from existing graph signals only
- Storing risk assessments — computed on-demand, not persisted for now

## Decisions

1. **Score derivation**: Combine three signals with weighted sum
   - drift_score (0-1, weight 0.4): From `edge_drift` table for symbol's outgoing edges
   - co_change_partner_count (0-N, normalized to 0-1, weight 0.3): Count of CoChangesWith edges / max_threshold
   - hotspot_score (0-1, weight 0.3): Recent touch frequency from git intelligence

2. **Risk thresholds**:
   - critical: score >= 0.8
   - high: score >= 0.6
   - medium: score >= 0.4
   - low: score < 0.4

3. **Source exclusively graph**: ChangeRiskCard does not read from overlay — all signals from graph tables.

4. **No new table**: Risk is computed on-demand. If future need arises, can add `risk_snapshot` table.

## Risks / Trade-offs

- [Risk]: Signals may be missing for some symbols (no drift, no co-change edges) → Partial scoring, show available signals in risk_factors
- [Risk]: Hotspot data depends on git_intelligence stage → Require stage 5 completion, degrade gracefully if unavailable
- [Trade-off]: On-demand computation adds latency → Compute lazily at deep budget only, or add risk field to SymbolCard/FileCard for caching