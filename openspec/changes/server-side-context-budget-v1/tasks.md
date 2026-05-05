## 1. Contracts and Docs

- [ ] 1.1 Add server-side context budget OpenSpec deltas.
- [ ] 1.2 Update `skill/SKILL.md` with the context budget contract.
- [ ] 1.3 Update MCP public and implementation docs with new defaults, caps, and resource examples.

## 2. MCP Runtime

- [ ] 2.1 Add shared MCP limit constants and bounded-limit helpers.
- [ ] 2.2 Add final response budget clamping and route tool responses through it.
- [ ] 2.3 Make search defaults compact, bounded, and guarded for cards mode.
- [ ] 2.4 Tighten card batch and context-pack defaults, caps, omitted metadata, and priority retention.
- [ ] 2.5 Add bounded limits and omissions to fan-out graph primitives.

## 3. Metrics

- [ ] 3.1 Extend context metrics with flood, deep-card, context-pack, and per-tool token counters.
- [ ] 3.2 Surface new metrics through MCP metrics, CLI stats, and Prometheus.

## 4. Validation

- [ ] 4.1 Add regression tests for search defaults, raw caps, cards-mode guardrails, card batch caps, context-pack caps, resources, response clamps, metrics privacy, and skill text.
- [ ] 4.2 Run focused MCP tests, lint, and OpenSpec status checks.
