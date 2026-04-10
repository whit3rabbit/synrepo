## Context

The runtime layout is already real enough to deserve policy. `synrepo init` creates `.synrepo/graph/`, `.synrepo/overlay/`, `.synrepo/index/`, `.synrepo/embeddings/`, `.synrepo/cache/llm-responses/`, and `.synrepo/state/`, then writes `config.toml`. The docs go further and define retention targets, compaction behavior, and the idea that stores should be versioned independently. What is missing is an active change that turns those expectations into a compatibility contract future implementation can follow.

This is the right next planning step because substrate, bootstrap, repair, exports, and watch/ops all depend on the same storage policy. If migration and rebuild rules stay implicit, later changes will each invent their own version of what `.synrepo/` means.

This is an early contract-sharpening change, not a milestone reorder. Implementation should still land in step with the roadmap sections that establish graph stores, watch/reconcile behavior, and later maintenance flows.

## Goals / Non-Goals

**Goals:**
- Define store ownership and compatibility rules for the current `.synrepo/` layout.
- Specify which stores are canonical, supplemental, cached, or disposable.
- Define rebuild versus migrate versus invalidate behavior at a policy level.
- Define which config changes should trigger warnings, rebuilds, migrations, or refusal.
- Give later maintenance commands and background operations a stable contract to implement.

**Non-Goals:**
- Implement full migration code for every future store format.
- Add all maintenance commands immediately.
- Change the graph versus overlay separation.
- Fold watch/ops, exports, or repair-loop entirely into one storage change.

## Decisions

1. `.synrepo/` stores have different durability classes.
   Graph and future canonical persisted stores are not treated like caches.
   Indexes, embeddings, and LLM caches may be rebuildable or evictable.
   State files and logs have their own retention and operational role.

2. Compatibility-sensitive config is explicit.
   Settings that change discovery, indexing semantics, Git-history depth, or persisted format expectations must have declared operational consequences.

3. Rebuild and migration are different outcomes.
   Some stores can be safely dropped and rebuilt, some require migration, and some may trigger refusal until the user runs a maintenance workflow. The contract should define which class each store belongs to.

4. Maintenance policy belongs here, execution belongs later.
   This change defines what later `compact`, cleanup, refresh, and upgrade flows must honor, without requiring all those commands to ship immediately.

5. The current runtime layout remains the baseline.
   This change should fit the directories and files the repo already creates or documents instead of inventing a second storage model.

## Risks / Trade-offs

- Being too vague here guarantees churn later, but being too prescriptive on future store internals would fake certainty. The contract should focus on compatibility behavior and store classes, not accidental implementation detail.
- If too many config fields become compatibility-sensitive, normal configuration will feel brittle. If too few do, rebuild and migration behavior will be surprising.
- Touching both `storage-and-compatibility` and `watch-and-ops` in one change is justified because retention and maintenance operations cross the boundary, but the change still needs to avoid becoming a generic ops umbrella.
