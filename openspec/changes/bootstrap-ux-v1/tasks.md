## 1. Define init flow and mode behavior

- [ ] 1.1 Implement repository inspection and mode-selection behavior that respects explicit `--mode` while supporting a defined default path
- [ ] 1.2 Add tests for auto-mode defaulting, curated-mode detection or recommendation, and explicit-mode override behavior
- [ ] 1.3 Align config and bootstrap code comments with the chosen mode-selection semantics

## 2. Implement health states and first-run output

- [ ] 2.1 Define and implement bootstrap outcome states such as healthy, degraded, and blocked
- [ ] 2.2 Update `synrepo init` output to include the chosen mode, runtime path, substrate/index status, and next-step guidance
- [ ] 2.3 Add CLI tests that assert the mandatory first-run summary for successful and degraded bootstrap outcomes

## 3. Define re-entry behavior

- [ ] 3.1 Decide and implement what `synrepo init` does when `.synrepo/` already exists, including clear user guidance
- [ ] 3.2 Add tests for already-initialized and partially initialized repository states
- [ ] 3.3 Validate the change with `openspec validate bootstrap-ux-v1 --strict --type change`
