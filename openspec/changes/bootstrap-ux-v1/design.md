## Context

Today `synrepo init` creates `.synrepo/`, writes `config.toml`, writes a `.gitignore`, and builds the initial index. That is a solid skeleton, but it is not yet a polished bootstrap UX. It assumes the user already knows which mode to pick, treats re-running init as a hard stop, and does not surface a meaningful health state or next action after the index build completes.

This is the right next planning change because bootstrap is part of the wedge, not polish. If first-run behavior is sloppy now, later graph and card features inherit a rough onboarding path and ambiguous operational semantics.

## Goals / Non-Goals

**Goals:**
- Define a concrete first-run UX for `synrepo init` that matches the current runtime shape but improves decision clarity.
- Specify how mode selection works when explicit flags, repository signals, and later detected rationale sources disagree.
- Define successful, degraded, and blocked bootstrap outcomes.
- Define what happens when init is invoked in an already initialized repository.
- Specify the minimum first-run summary that the CLI must emit after a successful bootstrap.

**Non-Goals:**
- Implement cards, MCP surfaces, repair commands, or daemon flows.
- Solve all long-term storage migration logic in this change.
- Add generated assistant shims beyond what the bootstrap spec requires for v1.
- Expand bootstrap into a generic project wizard.

## Decisions

1. Explicit user choice beats heuristic mode selection.
   If the user passes `--mode`, synrepo should honor it. Repository inspection may still emit a recommendation or warning, but it should not silently override the explicit choice.

2. Repository signals can drive the default path.
   If no explicit mode is provided, bootstrap should inspect configured rationale directories and related project signals to choose or recommend auto versus curated mode according to the bootstrap contract.

3. Bootstrap ends in a defined health state.
   Success is not just “directories were created.” The user should see whether bootstrap is healthy, degraded, or blocked, and the state should explain what was completed and what remains.

4. Re-entry behavior is explicit.
   Hard-failing on existing `.synrepo/` state is acceptable only if the command tells the user what to do next. If later bootstrap semantics support refresh or repair, that path should be named clearly rather than emerging accidentally.

5. First-run output is part of the product contract.
   A successful init should report the chosen mode, the created or reused runtime location, substrate status, and the next recommended command or workflow. Optional niceties can layer on top later.

## Risks / Trade-offs

- Heuristics that are too aggressive will surprise users; heuristics that are too timid will leave them with an unnecessary manual choice. The contract should prefer explicit override plus visible recommendation.
- Defining degraded and blocked states now adds some UX discipline cost, but it prevents “success” messages that hide a broken initial environment.
- Keeping re-entry semantics narrow in this change avoids stepping into the full repair-loop design, but it means some later workflows will still need dedicated follow-on commands.
