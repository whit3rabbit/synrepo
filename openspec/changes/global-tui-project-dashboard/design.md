## Context

The current dashboard is project-local by construction: bare `synrepo` probes one `repo_root`, creates one `AppState`, and all widgets/actions derive state from that root. Global MCP support already depends on `~/.synrepo/projects.toml`, but that registry is still mostly install bookkeeping: entries are canonical paths with install metadata, not stable user-facing projects.

The global dashboard should reuse existing repo-local `.synrepo/` stores and existing watch/control paths. The risky boundary is state ownership: switching `repo_root` inside the current `AppState` would leak cached snapshots, event receivers, materializer state, modals, logs, and explain previews across repositories.

## Goals / Non-Goals

**Goals:**

- Promote the existing registry into a stable project-selection source with backward-compatible IDs, display aliases, and recency metadata.
- Wrap the current project dashboard in a global shell that owns many project states and renders one active project.
- Keep every mutating or heavyweight action scoped to an explicit project identity.
- Provide fast project selection, safe detach/remove semantics, and all-project watch visibility.
- Improve keyboard safety and accessible rendering while preserving existing non-TTY and explicit `--repo` behavior.

**Non-Goals:**

- Moving graph/overlay databases out of per-repo `.synrepo/` storage.
- Adding a file/folder browser or automatic broad filesystem scan.
- Deleting project state from the project picker. Destructive cleanup remains in `synrepo remove` and `synrepo uninstall`.
- Auto-starting watch or materialization for background projects.
- Hosting MCP stdio inside the TUI.

## Decisions

### D1. Extend `projects.toml`, do not add a second registry

Add optional/defaulted fields to `ProjectEntry`: `id`, `name`, and `last_opened_at`. Existing entries remain valid. When `id` is absent, code computes an effective stable ID from the canonical path and writes it back only when the registry is next updated.

Alternative: add `~/.synrepo/projects.json`. Rejected because it would split registry truth and duplicate existing MCP/project-manager behavior.

### D2. Stable identity is not folder name

Project identity is a generated/derived ID. Display name defaults to the path basename and can be renamed independently. Name resolution is allowed for convenience, but ambiguous names must fail with matching IDs/paths.

Alternative: key projects by name. Rejected because duplicate repo names are normal (`~/code/synrepo`, `~/forks/synrepo`).

### D3. Add a global TUI shell around project-scoped state

Keep the current dashboard state project-scoped and introduce `GlobalAppState` with registry snapshots, active project ID, and `HashMap<ProjectId, ProjectRuntimeState>`. Switching active projects swaps project state and clears global transients atomically. It does not mutate another project.

Alternative: mutate `AppState.repo_root` in place. Rejected because too many fields are repo-specific and leakage would be subtle.

### D4. Action context carries project identity

TUI dispatch uses `ProjectActionContext { project_id, project_name, repo_root, synrepo_dir }`. The existing `ActionContext::new(repo_root)` remains for CLI/single-project call sites. Logs and toasts include project labels when actions may be observed in a global view.

### D5. Project picker is a registry selector, not a folder manager

The picker lists registered projects by recency, supports filter/switch/rename/add-cwd/detach, and displays health/watch/lock/integration state. Adding arbitrary scan/import can come later.

### D6. Accessibility is explicit UI configuration

Add a small accessibility settings value passed through TUI options and widgets. `--no-color` remains supported, while reduced-motion and ASCII rendering control spinner/fresh-row/glyph choices.

## Risks / Trade-offs

- [Risk] Registry schema expansion breaks old installs -> Mitigation: all new fields use serde defaults or aliases, and tests load legacy path-only entries.
- [Risk] Global dashboard accidentally mutates the wrong project -> Mitigation: action methods take `ProjectActionContext`, and switch tests assert pending state is cleared.
- [Risk] Watch visibility encourages background mutation -> Mitigation: background projects are read-only except explicit per-row watch start/stop.
- [Risk] Scope is large -> Mitigation: land registry/CLI, then project shell, then UX/accessibility in separable commits.
