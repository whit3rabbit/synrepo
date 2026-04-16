# Handoffs Surface

> Spec: `handoffs-surface`

## Overview

The handoffs surface is a derived read-only surface that aggregates repair recommendations, pending cross-link candidates, and git hotspot signals into prioritized actionable items. It is computed at query time and does not persist any new data.

## Capability ID

`handoffs-surface`

## Requirements

### Requirement: Handoffs CLI command
synrepo SHALL expose a `synrepo handoffs` CLI command that reads repair-log, overlay candidates, and git hotspots, then emits prioritized actionable items.

Parameters:
- `--json` — output as JSON instead of markdown table
- `--limit N` — limit to top N items (default 20)
- `--since DAYS` — only include items from the last N days (default 30)

### Requirement: Handoffs MCP tool
synrepo SHALL expose `synrepo_next_actions` as an MCP tool that returns prioritized actionable items.

Parameters:
- `limit` (optional): number of items to return (default 20)
- `since_days` (optional): only include items from the last N days (default 30)

### Requirement: Data sources
The handoffs surface SHALL read from:
- Repair-log: `.synrepo/state/repair-log.jsonl` — filter for unresolved items within the `since` window
- Overlay: pending cross-link candidates (status = `pending`)
- Git hotspots: top files by commit frequency in the `since` window (via existing git-intelligence query)

### Requirement: Priority ordering
Handoffs items SHALL be ordered by:
1. Severity class (repair severity: critical > high > medium > low)
2. Cross-link confidence (high > medium > low)
3. Recency (most recent first)
4. Surface type (structural surfaces before overlay)

### Requirement: Output format
The handoffs response SHALL include for each item:
- `id`: unique identifier for the item
- `type`: one of `repair`, `cross_link`, `hotspot`
- `source`: file path or symbol reference
- `recommendation`: actionable text
- `priority`: one of `critical`, `high`, `medium`, `low`
- `source_file`: file where the item originates
- `source_line`: line number (if applicable)

## Acceptance Criteria

- [ ] `synrepo handoffs` outputs a markdown table with prioritized items
- [ ] `synrepo handoffs --json` outputs valid JSON with the same items
- [ ] `synrepo handoffs --limit 5` limits output to 5 items
- [ ] `synrepo handoffs --since 7` filters to items from the last 7 days
- [ ] `synrepo_next_actions` MCP tool appears in the tool list
- [ ] `synrepo_next_actions` returns JSON matching the CLI JSON format
- [ ] Items are correctly prioritized by severity, confidence, and recency
