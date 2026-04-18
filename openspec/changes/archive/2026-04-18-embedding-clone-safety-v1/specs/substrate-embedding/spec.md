## MODIFIED Requirements

### Requirement: FlatVecIndex no longer implements Clone

FlatVecIndex SHALL NOT implement Clone. The `impl Clone for FlatVecIndex` deliberately dropped the embedding session on clone, creating a latent footgun. Since no production code relies on cloning, the impl is removed.

#### Scenario: Attempting to clone a FlatVecIndex

- **WHEN** code calls `.clone()` on a FlatVecIndex loaded via `load()`
- **THEN** the code fails to compile with "the trait `Clone` is not implemented for `FlatVecIndex`"

**Rationale:** Removing the impl eliminates the runtime footgun without needing Arc-wrapping or type splits. The `load_with_resolution()` path remains the correct way to get a session-enabled index.

### Requirement: embed_text error message simplified

The error message SHALL NOT mention "cloned indices" since cloning is no longer possible.

#### Scenario: Calling embed_text on session-less index

- **WHEN** code calls `index.embed_text("query")` on a FlatVecIndex loaded via `load()`
- **THEN** an error is returned: "Embedding session not available. Use load_with_resolution() to restore a session."

**Rationale:** The shortened message points users directly to the fix without confusing "cloned" framing.

### Requirement: Contract test removed

The `clone_drops_session_and_disables_embed_text` test SHALL be removed because it tests a behavior that no longer exists.

#### Scenario: Running embedding tests after change

- **WHEN** tests in `substrate::embedding::index::tests` are run
- **THEN** all tests pass, including `embed_text_fails_when_session_missing` which verifies the session-missing contract

**Rationale:** The remaining test `embed_text_fails_when_session_missing` continues to enforce the session requirement without depending on the removed Clone impl.