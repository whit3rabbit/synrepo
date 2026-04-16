# runtime-safety-hardening-v1

## Why

The current runtime safety model has two weaknesses:

1. Writer exclusivity is weaker than the product contract implies.
   The writer layer documents a single-writer contract for `.synrepo/` runtime state,
   but the implementation uses per-lock-path re-entrancy depth tracking.
   This is useful for nested same-process calls, but it weakens the guarantee that
   only one write execution path is active at a time.

2. Several runtime paths scale memory with repository size instead of with the
   requested result size.
   In particular, vector similarity query allocates and sorts a score for every
   chunk, and some overlay/status surfaces load full result sets before applying
   limits.

These weaknesses are manageable in small repos and simple CLI flows, but they
increase the risk of race conditions, long write critical sections, and avoidable
memory growth as repository size and overlay volume increase.

## What Changes

This change hardens runtime mutation and memory behavior by:

- making writer re-entrancy explicit and thread-scoped rather than process-wide
- introducing a single write-admission entry point that coordinates watch state
  and writer ownership
- reducing write critical-section duration where safe
- replacing full-score materialization in vector query with bounded top-k
  selection
- pushing overlay sorting/limit into storage queries where possible
- avoiding full-materialization status/report scans when a bounded or aggregate
  form is sufficient

## Non-Goals

- changing graph semantics
- changing overlay candidate scoring semantics
- introducing approximate nearest-neighbor search
- introducing multi-writer support
- redesigning watch mode beyond write-admission coordination