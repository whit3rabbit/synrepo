## 1. Registry And CLI

- [x] 1.1 Add backward-compatible `id`, `name`, and `last_opened_at` fields to project registry entries.
- [x] 1.2 Add project ID/name/path resolution helpers, duplicate-name ambiguity errors, display-name defaults, and last-opened update helpers.
- [x] 1.3 Add `synrepo project rename` and `synrepo project use` CLI commands and tests.
- [x] 1.4 Update project list/inspect output to include project ID, display name, and last-opened metadata.

## 2. Global Dashboard State

- [x] 2.1 Add project-facing TUI models for `ProjectRef`, `ProjectHealthSummary`, and project-scoped runtime state.
- [x] 2.2 Add `GlobalAppState` that loads registry projects, tracks the active project, and switches projects atomically.
- [x] 2.3 Keep current dashboard construction working for explicit single-project entrypoints while routing bare global entry through the new shell.

## 3. Project-Scoped Actions

- [x] 3.1 Add `ProjectActionContext` while preserving existing `ActionContext::new(repo_root)` compatibility.
- [x] 3.2 Route TUI reconcile, sync, watch, materialize, explain, auto-sync, docs export, and docs clean through explicit project context.
- [x] 3.3 Add project labels to global-visible logs and toasts.

## 4. Project Picker And Watch Visibility

- [x] 4.1 Add `[p]` project picker with filtering, switch, rename, add current directory, and detach selected project.
- [x] 4.2 Render project rows with display name, path, health, watch state, lock state, and integration state.
- [x] 4.3 Add watch manager behavior for start/stop per selected project without auto-starting background projects.

## 5. UX Safety And Accessibility

- [x] 5.1 Add help overlay and command palette entrypoint before adding further one-letter actions.
- [x] 5.2 Add confirm metadata to quick actions and confirmation flows for materialize, docs clean apply, and auto-sync in global mode.
- [x] 5.3 Preserve essential footer hints during toasts.
- [x] 5.4 Add no-color/reduced-motion/ASCII accessibility settings through TUI options and widgets.
- [x] 5.5 Fix confirm-stop-watch tab `5` fall-through and make Live pagination viewport-aware.

## 6. Verification

- [x] 6.1 Add registry compatibility, duplicate-name, rename, use, and detach tests.
- [x] 6.2 Add TUI switching/action-scope/project-picker/widget accessibility tests.
- [x] 6.3 Run targeted test commands from the plan and `make ci-lint`.
- [x] 6.4 Run `openspec status --change global-tui-project-dashboard --json` and confirm the change is apply-ready.

## 7. MCP And Explore Tabs

- [x] 7.1 Add MCP and Explore tab states, key bindings, footer/help text, and tab rendering.
- [x] 7.2 Add active-project MCP status resolution with scope, trigger/source, config path, and tests.
- [x] 7.3 Add registry-backed Explore tab selection, switching, refresh, watch toggle, and tests.
- [x] 7.4 Run targeted TUI/registry tests and lint verification.

## 8. Repo-Local MCP Install

- [x] 8.1 Route `i` from the MCP tab to an MCP-only project install picker while keeping generic integration on other tabs.
- [x] 8.2 Execute MCP-tab installs through `agent-config` local scope without writing shims, skills, instructions, or generic integration state.
- [x] 8.3 Add app-state, picker-state, and Codex executor tests for repo-local MCP behavior.
