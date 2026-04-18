## 1. Confirm current state

- [x] 1.1 Read `src/pipeline/structural/mod.rs` lines 1–30 and confirm the stale "Stages 5–8 … remain TODO stubs" phrasing is still present.
- [x] 1.2 Read `src/lib.rs` lines 1–30 and confirm it already accurately says only stage 8 is TODO.
- [x] 1.3 Check `src/lib.rs::docs_drift` (declared at `src/lib.rs:35-36`) to see whether any assertion locks the current stale phrasing. If yes, note which assertion needs updating alongside the docstring edit.

## 2. Rewrite the stage-status paragraph

- [x] 2.1 Replace the four-line block at `src/pipeline/structural/mod.rs:13-17` with the rewritten stage-5/6/7/8 description from design D1.
- [x] 2.2 Preserve every other line of the module docstring unchanged (relationship-to-watch section, observation-lifecycle section).
- [x] 2.3 Leave `src/lib.rs` untouched.

## 3. Update docs_drift test if needed

- [x] 3.1 If task 1.3 identified an assertion locking the stale phrasing, update the assertion to match the new docstring content.
- [x] 3.2 Otherwise skip this section.

## 4. Verification

- [x] 4.1 Run `cargo doc --no-deps --lib` and confirm no warnings from the changed file.
- [x] 4.2 Run `cargo test --lib docs_drift` (or the relevant test target if `docs_drift` is structured differently) and confirm it passes.
- [x] 4.3 Run `make check` and confirm fmt, clippy, and the full test suite pass.
- [x] 4.4 Spot-check by opening `src/pipeline/structural/mod.rs` in the IDE hover preview and confirming the stage status reads correctly.

## 5. Archive

- [ ] 5.1 Run `openspec validate docs-sync-pipeline-status-v1 --strict`.
- [ ] 5.2 Invoke `opsx:archive` with change id `docs-sync-pipeline-status-v1`.
