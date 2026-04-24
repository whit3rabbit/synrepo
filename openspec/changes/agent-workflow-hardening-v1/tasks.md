## 1. Doctrine And Tool Text

- [ ] 1.1 Update canonical agent doctrine with the required orient, find, impact or risks, edit, tests, changed sequence.
- [ ] 1.2 Add explicit full-file-read escalation language to generated shims.
- [ ] 1.3 Update MCP server info and relevant tool descriptions with short workflow guidance.

## 2. Metrics And Accounting

- [ ] 2.1 Add observable workflow counters for orient, find, explain, impact, risks, tests, changed, and minimum-context calls.
- [ ] 2.2 Add estimated cold-file-read avoidance derived from raw-file token comparisons where available.
- [ ] 2.3 Keep estimated counters separate from directly observed counters in JSON and text output.

## 3. Minimum Context Guidance

- [ ] 3.1 Update minimum-context docs or tool text to position it as the bounded neighborhood step before deeper inspection.
- [ ] 3.2 Add tests proving card and minimum-context responses retain accounting metadata needed for escalation decisions.
- [ ] 3.3 Verify no doctrine text suggests overlay notes or commentary are canonical source truth.

## 4. Verification

- [ ] 4.1 Run doctrine byte-identity and shim tests.
- [ ] 4.2 Run MCP tool-list or description tests.
- [ ] 4.3 Run context metrics tests for observed and estimated counters.
- [ ] 4.4 Run `openspec validate agent-workflow-hardening-v1`.
- [ ] 4.5 Run `openspec status --change agent-workflow-hardening-v1 --json` and confirm `isComplete: true`.
