# change-risk-card-v1 Tasks

## 1. Card Type Definition

- [x] 1.1 Add ChangeRiskCard type to `src/surface/card/types.rs`
- [x] 1.2 Implement RiskLevel enum in types.rs
- [x] 1.3 Add RiskFactor struct for contributing signals
- [x] 1.4 Add Budget impl for ChangeRiskCard (tiny/normal/deep tiers)

## 2. Card Compiler

- [x] 2.1 Add ChangeRiskCard compilation to `src/surface/card/compiler/`
- [x] 2.2 Implement drift score query from edge_drift table
- [x] 2.3 Implement co-change partner count from CoChangesWith edges
- [x] 2.4 Add hotspot score from git-intelligence payloads
- [x] 2.5 Wire risk scoring algorithm (weighted sum per design)

## 3. MCP Tool

- [x] 3.1 Add synrepo_change_risk tool to `src/surface/mcp/`
- [x] 3.2 Implement symbol target resolution
- [x] 3.3 Implement file target resolution
- [x] 3.4 Add error handling for missing targets

## 4. CLI Surface

- [x] 4.1 Add `synrepo change-risk` command to CLI
- [x] 4.2 Wire JSON/text output formatting
- [x] 4.3 Add integration test for CLI command

## 5. Integration

- [x] 5.1 Add ChangeRiskCard to card export in `synrepo export`
- [x] 5.2 Verify budget tier behavior across all card types
- [x] 5.3 Run full test suite

## 6. Documentation

- [x] 6.1 Update cards spec with ChangeRiskCard delta
- [x] 6.2 Update MCP surface spec with synrepo_change_risk tool
- [x] 6.3 Update AGENTS.md shipped surfaces