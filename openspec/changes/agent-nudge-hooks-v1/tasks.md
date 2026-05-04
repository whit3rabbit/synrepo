## 1. OpenSpec and Contracts

- [x] 1.1 Create proposal, design, and delta specs for agent nudge hooks and doctrine updates.

## 2. Hook Runtime

- [x] 2.1 Add hidden CLI args and dispatch for `synrepo agent-hook nudge --client <client> --event <event>`.
- [x] 2.2 Implement pure prompt and tool classifiers, including RTK prefix stripping.
- [x] 2.3 Implement Codex and Claude nudge renderers that emit client-valid non-blocking output.

## 3. Setup Integration

- [x] 3.1 Add `--agent-hooks` to scripted setup for Codex and Claude.
- [x] 3.2 Add the explicit “Install nudge hooks” action to the TUI integration wizard.
- [x] 3.3 Implement safe local config merge for `.codex/hooks.json` and `.claude/settings.local.json`.
- [x] 3.4 Print the exact Codex `[features] codex_hooks = true` requirement when hook support is unavailable.

## 4. Docs and Validation

- [x] 4.1 Update canonical doctrine, generated shims, `skill/SKILL.md`, and `docs/MCP.md`.
- [x] 4.2 Add classifier, renderer, installer merge, and CLI smoke tests.
- [x] 4.3 Run focused tests and `cargo clippy --workspace --bins --lib -- -D warnings`.
