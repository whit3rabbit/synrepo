## MODIFIED Requirements

### Requirement: Surface context metrics in operator views
The shared status snapshot SHALL include context metrics so `synrepo status --json` and the dashboard can report card usage and context savings without duplicating logic. The dashboard Health tab SHALL render, at minimum: cards served, average tokens per card, tokens avoided (estimated raw-file tokens saved), and stale responses. The stale-responses row SHALL use the `Stale` severity when the counter is greater than zero so operators notice accumulating advisory staleness without reading the full JSON snapshot.

#### Scenario: Dashboard renders context metrics
- **WHEN** the dashboard opens on a ready repository with non-empty context metrics
- **THEN** the Health tab displays rows for cards served, average card tokens, tokens avoided, and stale responses
- **AND** the stale-responses row is elevated to `Stale` severity when the count is greater than zero

#### Scenario: Context metrics absent
- **WHEN** no context metrics have been recorded yet
- **THEN** the context, tokens-avoided, and stale-responses rows are omitted rather than rendered as zero
- **AND** the remaining Health rows render unchanged
