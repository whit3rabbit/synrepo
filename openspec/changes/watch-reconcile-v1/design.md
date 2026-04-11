## Context

`structural-graph-v1` gave synrepo a persisted graph store and inspection surface, and `structural-pipeline-v1` is now the follow-on change that makes graph population automatic during bootstrap and refresh. The next operational gap after that is staying current under ordinary developer churn. The repo already documents the intent clearly in `openspec/specs/watch-and-ops/spec.md`, `ROADMAP.md`, and the foundation docs: watcher misses must not silently poison state, the daemon should remain optional, and writer ownership must stay explicit.

This change should therefore be planning-ready now, but sequenced after `structural-pipeline-v1`. Without automatic graph producers, watch and reconcile behavior would have nothing trustworthy to drive. With them, watch/reconcile becomes the next trust feature rather than background polish.

## Goals / Non-Goals

**Goals:**
- Define the first implementation-ready watcher and reconcile loop for keeping substrate and graph state fresh after local repository changes.
- Define single-writer operational safety for standalone CLI and future daemon-assisted operation.
- Define the initial operational diagnostics and failure-recovery surface so stale state is observable rather than mysterious.
- Define a narrow first maintenance and cleanup slice that consumes the existing storage-compatibility contract.

**Non-Goals:**
- Implement the structural producers themselves, which belong to `structural-pipeline-v1`.
- Implement overlay refresh orchestration, commentary freshness policies, or MCP server lifecycle in full.
- Build every future ops command or long-term retention mechanism in one pass.
- Change graph semantics, cards, or Git-intelligence ranking behavior.

## Decisions

1. Sequence this change after `structural-pipeline-v1`.
   Watching and reconcile only matter once there is a deterministic structural compile worth rerunning. The roadmap should say that plainly.
   Alternative considered: keep watch/reconcile as the immediate next change. Rejected because it puts ops ahead of the producer path it needs to supervise.

2. Start with single-writer safety, not full daemon dependence.
   The first implementation should define one authoritative writer at a time, with explicit locking for standalone operation and a clear handoff model for future daemon-assisted mode.
   Alternative considered: require a daemon from the start. Rejected because the foundation docs explicitly keep daemon usage optional where possible.

3. Reconcile is the correctness backstop for watcher misses.
   The watcher should be treated as a latency optimization, not the only source of truth. Periodic or startup reconcile passes must correct dropped or coalesced events.
   Alternative considered: trust file events alone. Rejected because the docs already call missed events under load a known risk.

4. The first diagnostics surface should stay small and operator-facing.
   Health or status output should explain stale state, locking conflicts, recent reconcile outcomes, and whether maintenance is needed, without inventing a giant ops dashboard.
   Alternative considered: postpone diagnostics until later. Rejected because opaque background behavior is exactly what this change is meant to prevent.

5. Maintenance behavior should consume, not replace, the storage-compatibility contract.
   Cleanup, compaction, and rebuild actions should respect the durability and compatibility classes already defined in `storage-compatibility-v1`.
   Alternative considered: let watch/reconcile invent its own retention rules. Rejected because that would duplicate and weaken the compatibility contract.

## Risks / Trade-offs

- Starting watcher work too early could blur the boundary with structural producers, mitigation: keep this change explicitly downstream of `structural-pipeline-v1`.
- Locking models are easy to get subtly wrong, mitigation: define the ownership contract clearly before implementation and keep the first write model simple.
- Reconcile intervals and event coalescing can become premature tuning work, mitigation: define the behavior first and leave aggressive optimization for later measurement.
- Maintenance commands can sprawl into a generic ops bucket, mitigation: keep the first slice limited to behavior the current storage contract already implies.

## Migration Plan

1. Keep `watch-reconcile-v1` planning-ready while `structural-pipeline-v1` lands automatic graph population.
2. Implement single-writer safety, watcher coalescing, and reconcile triggers once structural compile exists as a dependable operation.
3. Add operational diagnostics and narrow maintenance hooks that consume the storage-compatibility decisions already defined.
4. Follow later with deeper daemon, cleanup, and overlay-related ops work only if the first slice proves insufficient.

## Open Questions

- Whether the first operational status surface belongs in `synrepo status`, bootstrap health, or both.
- Whether reconcile should run on startup only at first, or also on a fixed interval in the initial slice.
