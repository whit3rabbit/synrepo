## 1. Confirm no hidden Clone bound

- [x] 1.1 Run `cargo check --workspace --all-targets` on a clean tree and confirm baseline compile is green.
- [x] 1.2 Run `rg -n 'Arc<FlatVecIndex>' src/` and confirm zero matches.
- [x] 1.3 Run `rg -n '(\\.clone\\(\\)|impl.*Clone)' src/substrate/embedding/` and confirm the only `FlatVecIndex::clone()` call is the contract test at `src/substrate/embedding/index.rs:510`.
- [x] 1.4 Run `rg -n 'FlatVecIndex' src/` and confirm no `#[derive(Clone)]` in the surrounding module chain relies on `FlatVecIndex: Clone`.

## 2. Delete the Clone impl

- [x] 2.1 Remove `impl Clone for FlatVecIndex { ... }` at `src/substrate/embedding/index.rs:33-50`.
- [x] 2.2 Run `cargo check --workspace --all-targets`. Fix any compile error — if a caller outside the tests surfaces, stop and re-scope the change; otherwise proceed.

## 3. Shorten the embed_text error message

- [x] 3.1 Edit the `anyhow::anyhow!(...)` at `src/substrate/embedding/index.rs:124-128` to drop the "Cloned indices cannot embed text" clause, leaving guidance pointing only at `load_with_resolution()`.
- [x] 3.2 Re-run `cargo check --workspace --all-targets`.

## 4. Update tests

- [x] 4.1 Delete `clone_drops_session_and_disables_embed_text` at `src/substrate/embedding/index.rs:499-518`.
- [x] 4.2 Edit `embed_text_fails_when_session_missing` (`src/substrate/embedding/index.rs:480-497`): update its assertion message to match the new error text from task 3.1 (look for the `load_with_resolution` substring rather than the `cloned` substring).
- [x] 4.3 Run `cargo test --lib substrate::embedding::` and confirm the remaining tests pass.

## 5. Verification

- [x] 5.1 Run `make check` and confirm fmt, clippy (product targets), and the full test suite pass.
- [x] 5.2 Run `cargo clippy --workspace --all-targets -- -D warnings` (broader than CI gate) and confirm no new lints from the change.
- [x] 5.3 Smoke-test: `cargo run -- init` against a small repo that exercises embedding, confirm indices build and query without regression. Run `cargo run -- search <query>` and confirm results return.

## 6. Archive

- [x] 6.1 Run `openspec validate embedding-clone-safety-v1 --strict`.
- [x] 6.2 Invoke `opsx:archive` with change id `embedding-clone-safety-v1`.
