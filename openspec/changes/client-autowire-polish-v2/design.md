## Context

Synrepo supports multiple agent targets and is gaining multi-client setup invocation. The rough edge is not capability coverage, it is explainability: users need to know which clients were found and what happened to each config or shim.

## Goals / Non-Goals

**Goals:**

- Make setup output auditable per client.
- Preserve non-destructive behavior for existing shims unless `--regen` is used.
- Clarify project versus global writes.
- Keep generated doctrine text sourced from the canonical doctrine block.

**Non-Goals:**

- No new client target in this change.
- No CI or release workflow changes.
- No overwrite of user-authored config without an existing approved setup path.

## Decisions

1. **Report every resolved target.** Multi-client commands should list each selected, skipped, detected, or failed target so users do not infer from absence.

2. **Keep detection observational.** Detection chooses defaults and reports host signals. It does not mutate until the setup command reaches the existing write step.

3. **Separate shim freshness from MCP registration.** A shim can be current while MCP registration is missing, or vice versa. The report must show both.

4. **Reuse `--regen` for stale shims.** Stale generated shims are reported with guidance; they are not overwritten silently.

## Risks / Trade-offs

- More detailed output can be noisy, mitigation: compact summary first, details only per target.
- Detection can be imperfect, mitigation: label signals as detected, not guaranteed installed.
- Global config writes are higher risk, mitigation: report scope and path before or during the existing confirmation surface.
