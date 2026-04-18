## Why

`FlatVecIndex::clone()` at `src/substrate/embedding/index.rs:33-50` deliberately drops the `EmbeddingSession` (`session: None` on line 47). Cloned instances then fail with a runtime error at `embed_text()` (lines 122–135) — a latent footgun that the type system does not prevent.

A repo-wide grep confirms there are **no production call sites** for `FlatVecIndex::clone()`. The only caller is the `clone_drops_session_and_disables_embed_text` contract test at `src/substrate/embedding/index.rs:500-518`, which exists only to pin the footgun in place. There is no `#[derive(...)]` or trait bound anywhere in the codebase that requires `FlatVecIndex: Clone`.

A `Clone` impl whose only purpose is to be tested for its own error message is dead weight. Removing it eliminates the footgun without needing runtime guards, Arc-wrapping, or a type split, and the shipped code gets smaller and easier to reason about.

## What Changes

- Delete `impl Clone for FlatVecIndex` at `src/substrate/embedding/index.rs:33-50`.
- Delete the `clone_drops_session_and_disables_embed_text` test at `src/substrate/embedding/index.rs:499-518` — it exists solely to assert the current Clone behaviour.
- Keep `embed_text_fails_when_session_missing` at `src/substrate/embedding/index.rs:480-497`. It still locks the correct session-missing contract for indices loaded without a resolution (the `load()` path sets `session: None`; see `load_with_resolution` vs `load`).
- Update the `embed_text` error message at `src/substrate/embedding/index.rs:124-128` to drop the "Cloned indices cannot embed text" framing, since clone is no longer a possible source of the missing session. Point users only at `load_with_resolution()`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None. The substrate embedding behaviour is unchanged from a consumer's perspective; `Clone` was never relied on.

## Impact

- **Code**: `src/substrate/embedding/index.rs` — remove Clone impl, remove one test, update one error message.
- **APIs**: `FlatVecIndex: Clone` is removed. External callers using `synrepo` as a library lose the ability to call `.clone()` on the index; a grep over `pipeline::synthesis::cross_link::triage::semantic` and `pipeline::diagnostics` (the two production consumers) confirms neither does.
- **Dependencies**: None.
- **Systems**: None. No serialization, storage, or runtime behaviour change.
- **Docs**: Inline docstring on `embed_text` becomes slightly shorter.
