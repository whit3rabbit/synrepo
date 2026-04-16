# change-risk-card-v1 Proposal

## Why

Today, agents must manually correlate drift scores, co-change edges, and git hotspots to estimate the risk of making a change. This requires multiple queries and cross-referencing data from the graph. Agents would benefit from a single card that surfaces change risk directly from existing structural data.

## What Changes

- **New card type**: `ChangeRiskCard` — aggregates drift, co-change, and hotspot data into a risk score
- **New MCP tool**: `synrepo_change_risk` — returns the card for a given symbol or file
- **New CLI**: `synrepo change-risk <target>` — CLI surface for the card

## Capabilities

### New Capabilities

- `change-risk-card`: ChangeRiskCard consuming drift scores, co-change edges, and git hotspot data to produce a risk assessment. Fields: risk_level (low|medium|high|critical), drift_score, co_change_partner_count, recent_hotspot_score, risk_factors (list of contributing signals), affected_revisions.
- `change-risk-mcp-tool`: synrepo_change_risk MCP tool that returns a ChangeRiskCard for a symbol or file target.

### Modified Capabilities

- `cards`: Adds new ChangeRiskCard to the card contract.
- `mcp-surface`: Adds new synrepo_change_risk tool.

## Impact

- New card type in `src/surface/card/types.rs` and `src/surface/card/compiler/`
- New MCP tool in `src/surface/mcp/tools/change_risk.rs`
- New CLI command in `src/bin/cli.rs`
- Changes the cards spec to document the new card type