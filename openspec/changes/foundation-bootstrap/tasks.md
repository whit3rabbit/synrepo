## 1. Establish the durable OpenSpec spine

- [x] 1.1 Create the capability directories under `openspec/specs/` for foundation, substrate, graph, cards, mcp-surface, bootstrap, patterns-and-rationale, repair-loop, watch-and-ops, overlay, evaluation, git-intelligence, storage-and-compatibility, and exports-and-views
- [x] 1.2 Write concise durable requirements and scenarios for each capability based on `ROADMAP.md`, `docs/FOUNDATION.md`, and `docs/FOUNDATION-SPEC.md`
- [x] 1.3 Confirm each spec owns one contract boundary and does not drift into runtime implementation detail

## 2. Define bootstrap governance

- [x] 2.1 Write `foundation-bootstrap/proposal.md` to explain why the foundation pass exists and what it changes
- [x] 2.2 Write `foundation-bootstrap/design.md` to lock repository boundaries, naming rules, and future change sequencing
- [x] 2.3 Write `foundation-bootstrap/tasks.md` as the actionable setup checklist for the planning layer
- [x] 2.4 Add delta specs for `foundation`, `bootstrap`, and `evaluation` that establish the initial governance behavior introduced by this change

## 3. Tighten project-level OpenSpec configuration

- [x] 3.1 Add synrepo-specific context to `openspec/config.yaml`
- [x] 3.2 Add artifact rules that keep future OpenSpec work foundation-first, roadmap-aligned, and clearly separated from runtime product behavior

## 4. Validate and hand off

- [x] 4.1 Run `openspec list --specs` and `openspec list` to confirm the workspace recognizes the new specs and active bootstrap change
- [x] 4.2 Run `openspec validate --specs --strict` and `openspec validate foundation-bootstrap --strict --type change`
- [x] 4.3 Ensure the next implementer can open `lexical-substrate-v1` or `bootstrap-ux-v1` without re-deciding folder layout, naming, or spec ownership
