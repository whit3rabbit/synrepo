## 1. Doctrine And Tool Text

- [x] 1.1 Update canonical agent doctrine with the required orient, find, impact or risks, edit, tests, changed sequence.
- [x] 1.2 Add explicit full-file-read escalation language to generated shims.
- [x] 1.3 Update MCP server info and relevant tool descriptions with short workflow guidance.

## 2. Metrics And Accounting

- [x] 2.1 Add observable workflow counters for orient, find, explain, impact, risks, tests, changed, and minimum-context calls.
- [x] 2.2 Add estimated cold-file-read avoidance derived from raw-file token comparisons where available.
- [x] 2.3 Keep estimated counters separate from directly observed counters in JSON and text output.

## 3. Minimum Context Guidance

- [x] 3.1 Update minimum-context docs or tool text to position it as the bounded neighborhood step before deeper inspection.
- [x] 3.2 Add tests proving card and minimum-context responses retain accounting metadata needed for escalation decisions.
- [x] 3.3 Verify no doctrine text suggests overlay notes or commentary are canonical source truth.

## 4. Verification

- [x] 4.1 Run doctrine byte-identity and shim tests.
- [x] 4.2 Run MCP tool-list or description tests.
- [x] 4.3 Run context metrics tests for observed and estimated counters.
- [x] 4.4 Run `openspec validate agent-workflow-hardening-v1`.
- [x] 4.5 Run `openspec status --change agent-workflow-hardening-v1 --json` and confirm `isComplete: true`.
