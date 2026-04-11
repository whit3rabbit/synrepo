## 1. Tighten the contract first

- [x] 1.1 Update `ROADMAP.md` to make `storage-compatibility-v1` the active runtime layout and versioning hardening slice
- [x] 1.2 Update the storage and ops specs to classify current stores, define per-store compatibility actions, and group compatibility-sensitive config fields
- [x] 1.3 Rewrite this change's proposal, design, and tasks so the implementation order matches the thin runtime compatibility layer

## 2. Add the thin runtime compatibility layer

- [x] 2.1 Add a shared compatibility policy module under `src/store/` with store identifiers, store classes, compatibility actions, and compatibility reporting
- [x] 2.2 Persist a machine-written compatibility snapshot under `.synrepo/state/storage-compat.json` with expected store-format versions and config-derived compatibility fingerprints
- [x] 2.3 Define compatibility checks for current stores so bootstrap and substrate no longer own this policy separately

## 3. Align config and CLI behavior

- [x] 3.1 Classify current config fields by compatibility impact, including index-sensitive, future graph or overlay-sensitive, and operational-only settings
- [x] 3.2 Route `synrepo init` and `synrepo search` through the shared compatibility checks and surface explicit rebuild, invalidate, migrate-required, or block guidance
- [x] 3.3 Preserve canonical stores on incompatibility and only auto-recreate disposable or ephemeral stores

## 4. Verify decisions and change validity

- [x] 4.1 Add tests for safe-continue, rebuild-required, invalidate, clear-and-recreate, migrate-required, and blocked compatibility outcomes
- [x] 4.2 Add CLI-facing tests for fresh init, unchanged rerun, compatibility-sensitive config drift, and missing or incompatible index state
- [x] 4.3 Validate the change with `openspec validate storage-compatibility-v1 --strict --type change`
