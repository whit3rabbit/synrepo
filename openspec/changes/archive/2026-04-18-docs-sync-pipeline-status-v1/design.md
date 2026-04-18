## Context

Module docstrings in `src/pipeline/structural/mod.rs` and `src/lib.rs` are the first surfaces a contributor reads when exploring the structural pipeline. Both exist today, but they disagree:

- `src/lib.rs:13-15` correctly states that stage 7 is implemented and stage 8 is the only remaining TODO.
- `src/pipeline/structural/mod.rs:14-17` states that stages 5–8 are all TODO stubs — wrong on stages 5, 6, and 7.

The inconsistency is purely documentation debt left over from the pre-`git-intelligence-v1` era. `AGENTS.md` / `CLAUDE.md` already document stages 5–7 as shipped (lines 173–176 of `AGENTS.md`), so the in-tree comment is the last stale surface.

## Goals / Non-Goals

**Goals:**

- Align `src/pipeline/structural/mod.rs` docstring with the current shipped stage set.
- Preserve the existing structure of the docstring (numbered stage list, relationship-to-watch-and-reconcile section, observation-lifecycle section).
- Keep the tone and voice consistent with sibling module docstrings under `src/pipeline/`.

**Non-Goals:**

- No changes to `src/lib.rs` — it is already accurate, and editing it would introduce churn for no gain.
- No changes to the `git_cache/mod.rs` docstring. Phase 1 exploration confirmed its HEAD-change-invalidation claim is consistent with the current code (cache is FIFO-bounded per path with HEAD-debounced invalidation; the separate `git-cache-breakage-invalidation-v1` change will extend this, not contradict it).
- No updates to `AGENTS.md` or `CLAUDE.md`. They are already accurate.
- No changes to `openspec/specs/` enduring specs. This is a code-comment change, not a behaviour change.
- No renumbering of stages.

## Decisions

### D1: Rewrite the stage-status paragraph, not the whole docstring

Replace the block at `src/pipeline/structural/mod.rs:13-17`:

```
//! Stage 4 (cross-file edge resolution) is now wired: after stages 1–3
//! commit, a name-resolution pass emits `Calls` and `Imports` edges.
//! Stages 5–8 (git mining, identity cascade, drift scoring, ArcSwap commit)
//! remain TODO stubs.
```

with the accurate status:

```
//! Stage 4 (cross-file edge resolution) is wired: after stages 1–3 commit,
//! a name-resolution pass emits `Calls` and `Imports` edges (TS/TSX, Python,
//! Rust, Go).
//! Stage 5 (git mining) is wired via `pipeline::git` and
//! `pipeline::git_intelligence`, emitting `CoChangesWith` edges and
//! per-file history/hotspot/ownership insights.
//! Stage 6 (identity cascade — content-hash rename, split/merge, git
//! rename fallback) is wired in `structure::identity`.
//! Stage 7 (drift scoring via Jaccard distance on persisted structural
//! fingerprints) is wired; sidecar `edge_drift` and `file_fingerprints`
//! tables hold the output.
//! Stage 8 (ArcSwap commit) remains TODO — see
//! `pipeline-stage8-arcswap-v1`.
```

**Rationale**. Smallest edit that makes the docstring accurate. The rest of the module doc (relationship-to-watch, observation-lifecycle) is still correct; do not touch it.

**Alternatives considered:**
- *Rewrite the whole docstring to match the style of newer modules*: rejected — drive-by churn, violates the "smallest change that satisfies the request" principle.
- *Delete the stage-list entirely and point to `AGENTS.md`*: rejected — the stage list is load-bearing for contributors reading the pipeline in isolation. Cross-references become stale too; local truth is better.

### D2: Leave `src/lib.rs` alone

`src/lib.rs:13-15` says: "`drift` scores per-edge Jaccard distance over persisted structural fingerprints (stage 7 — implemented, sidecar `edge_drift` / `file_fingerprints` tables). Stage 8 (ArcSwap commit) is still a TODO." This is already accurate. Editing it would either introduce style churn or duplicate the detailed stage-5/6/7 list that now belongs in `structural/mod.rs`. Leave it.

### D3: Do not add a cross-reference to the `pipeline-stage8-arcswap-v1` change

The proposal mentions the follow-on change name. In the docstring itself, the cross-reference is fine (contributors will search for the change when they hit the TODO). No need to also reference it from `src/lib.rs` or `AGENTS.md`.

## Risks / Trade-offs

- **Docs drift again the moment stage 8 lands**: accepted. The `pipeline-stage8-arcswap-v1` change will update the same docstring as part of its own work. This is the normal doc-update pattern, not a risk specific to this change.

- **Test surface: `src/lib.rs::docs_drift` module exists** (`src/lib.rs:35-36`). If that test locks any docstring content, the edit might break it. Verification task covers this.

## Migration Plan

Single commit. No migration.

## Open Questions

None.
