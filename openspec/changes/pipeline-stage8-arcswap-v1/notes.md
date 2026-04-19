## Approval Notes

- 2026-04-18: Operator confirmed the snapshot scope is the full in-memory graph, not a file-plus-symbol-only index.
- 2026-04-18: Operator approved lowering the default snapshot memory ceiling from the proposal's 500 MB to 128 MB.
- 2026-04-18: Verified on branch `main` that prerequisite `symbol-body-hash-column-v1` is already shipped.
  Evidence: commit `a18755a3f4df69c586f99610facf8f9d9558eae3` archives the change and the live codebase contains the dedicated `body_hash` column migration and downstream usage.
