## Context

`FlatVecIndex` wraps a flat vector store plus an optional `EmbeddingSession` used at query time to embed an incoming text query before similarity search. The session holds an ONNX Runtime handle and model weights; it is loaded from disk on `build()` and `load_with_resolution()`, but deliberately omitted on the cheap `load()` path that only deserializes vectors for lookup.

A manual `impl Clone` at lines 33–50 copies chunks and vectors but sets `session: None` in the clone. `embed_text()` (lines 122–135) explicitly checks for the missing session and returns an error naming `load_with_resolution()` as the recovery path. The docstring on `Clone::clone` warns about this; the type system does not.

Phase 1 exploration established:

- Zero production call sites for `FlatVecIndex::clone()`.
- No `#[derive(Clone)]` in the module depends on it.
- No trait bound in the codebase (`where T: Clone`, `: Clone`, etc.) requires `FlatVecIndex: Clone`.
- One contract test (`clone_drops_session_and_disables_embed_text`) exists for the sole purpose of pinning the current footgun behaviour.

The previously considered approaches — Arc-wrapping the session for shared-clone use, or splitting into `FlatVecIndexStorage` vs `FlatVecIndexQuery` types — solve a problem that is not being exercised. Removing the Clone impl outright is the smallest change that closes the footgun.

## Goals / Non-Goals

**Goals:**

- Make it a compile-time error to clone a `FlatVecIndex`, eliminating the runtime footgun.
- Preserve the existing `session: Option<EmbeddingSession>` field and the `embed_text` error path for the legitimate case: an index loaded via `load()` without a resolution.
- Keep `FlatVecIndex` construction, save, load, and similarity-search surfaces unchanged.

**Non-Goals:**

- No Arc-wrapping of the embedding session. The session is not currently shared across components; speculative concurrency support is out of scope.
- No type-level split between storage-only and query-capable index variants. YAGNI — nobody is asking for storage-only handles.
- No refactoring of the `load` vs `load_with_resolution` distinction. That distinction is intentional and well-used by `pipeline::diagnostics` (which calls `load` for introspection without needing query capability).
- No change to the on-disk index format (`INDEX_FORMAT_VERSION = 2`).
- No deprecation period. `Clone` was never part of an external contract; removing it is a hard break only for anyone who was relying on the latent footgun, and the grep confirms no one is.

## Decisions

### D1: Delete the Clone impl rather than convert to `Arc<EmbeddingSession>`

Two alternatives were considered:

- **Arc-wrap**: change `session: Option<EmbeddingSession>` to `session: Option<Arc<Mutex<EmbeddingSession>>>`. Adds a second lock on the query path for no proven consumer. The `EmbeddingSession` API currently takes `&self` on `embed` (see `embed` at `src/substrate/embedding/model.rs`), so `Mutex` would be a downgrade for no benefit unless a real multi-owner case appears.
- **Type split**: introduce `FlatVecIndexStorage` (clone-capable, no `embed_text`) separate from `FlatVecIndex`. Expands the public type surface and forces every existing consumer to know which one they want. No current consumer would pick storage-only.

Neither solves a live problem. **Delete the Clone impl** is the choice.

**Rationale**. Chesterton's fence says investigate before removing; here, the investigation shows the fence is guarding nothing. The only caller is the contract test. Deleting both the impl and the test keeps the module smaller and eliminates the runtime error path that only existed to catch misuse of this impl.

### D2: Keep `session: Option<EmbeddingSession>` and the existing `embed_text` error path

The `None` state is still reachable via `load()` (non-resolution variant used by diagnostics). The error at lines 123–128 remains load-bearing for that path. Shorten the message — "Cloned indices cannot embed text" is no longer accurate after this change.

Proposed new message:

> Embedding session not available. This index was loaded without a model resolution; call `load_with_resolution()` or `build()` to obtain a query-capable index.

### D3: Remove the contract test that exists only to pin current behaviour

`clone_drops_session_and_disables_embed_text` at lines 499–518 becomes dead code once `Clone` is gone. Delete it rather than repurpose it.

`embed_text_fails_when_session_missing` at lines 480–497 still exercises the `session: None` path via direct struct construction. Keep it; update the assertion string to accept the new message from D2 (`expected error explaining missing-session contract`).

## Risks / Trade-offs

- **External library consumer was relying on `FlatVecIndex: Clone`**: unlikely given the clone-drops-session footgun, and unverified externally. Mitigation: call out the breaking change in the change's commit message. The v1 suffix in the change name signals that future behaviour changes may follow.

- **Some unknown macro or derive expands to a `Clone` bound**: the grep was manual and may miss macro-generated bounds. Mitigation: `cargo check --workspace --all-targets` will hard-fail if anything expands to require `FlatVecIndex: Clone`. Verification task runs this.

- **`pipeline::synthesis::cross_link::triage::semantic` holds `&FlatVecIndex`** and may some day want to pass an owned index across threads: not today's problem. When it is a problem, the fix is `Arc<FlatVecIndex>`, which does not need `Clone` on `FlatVecIndex` itself.

## Migration Plan

Single commit. No migration. Library-consumer breakage (if any) surfaces at compile time.

## Open Questions

None.
