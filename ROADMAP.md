# ROADMAP.md

# synrepo roadmap using an OpenSpec-style planning system

## 0. Current status

- Milestone 0, Foundation setup: complete
- Milestone 1, First-run value: complete
- Milestone 2, Observed-facts core: in progress
- Most recently completed implementation change: `structural-graph-v1`
- Completed in the current milestone: `structural-graph-v1`
- Current Milestone 2 follow-on focus: `structural-pipeline-v1`, automatic graph population from repository state
- Early contract-sharpening change already opened for a later milestone: `git-intelligence-v1`

## 1. Purpose

This roadmap adapts synrepo to an OpenSpec-style workflow without turning specs into the runtime product.

OpenSpec is strongest when it is used as a change-management and artifact workflow: a stable `openspec/specs/` tree for enduring system behavior, plus `openspec/changes/<change>/` folders for proposals, delta specs, design, and tasks. synrepo should use that structure for planning and change control, while keeping the **graph** as runtime truth and **cards** as the product interface. The spec system governs intent and delivery. The graph governs what was directly observed in the repository.

This preserves the core synrepo thesis:

- observed facts only in the graph
- machine-authored content only in the overlay
- cards are the primary user-facing abstraction
- Phase 2, not overlay synthesis, is the first real ship target

## 2. Planning assumptions

### 2.1 Hard invariants

These invariants should shape every roadmap item and every future OpenSpec change:

1. The graph stores only parser-observed, git-observed, and human-declared facts.
2. The overlay stores machine-authored commentary, proposed links, and findings.
3. The synthesis pipeline never consumes overlay output as future input.
4. The system returns the smallest truthful context first.
5. The product is useful before any LLM synthesis exists.

### 2.2 What OpenSpec means for synrepo

For synrepo, OpenSpec is a **human planning layer**, not the live retrieval layer.

That means:

- `openspec/specs/` describes intended product behavior, contracts, and boundaries.
- `openspec/changes/` captures proposed work as proposal, delta specs, design, and tasks.
- `.synrepo/` remains the live runtime system for indexing, graph state, overlay state, caches, and metrics.
- Generated docs or agent shims may exist, but they are exports from the product, not the product itself.

## 3. Foundation strategy

The roadmap starts with foundation because synrepo has two risks that need to be controlled early:

1. **Architecture risk**: building too much planning machinery before the structural core exists.
2. **Adoption risk**: building a strong engine that is too hard to install, route, verify, and keep healthy.

The roadmap therefore merges the current synrepo phases with the best ideas worth borrowing from mex:

- **bootstrap UX**
- **targeted repair loop**
- **pattern surface**
- **watch and reconcile operations**
- **instability handling** for renames, splits, merges, stale overlay, and drift

These are treated as first-class roadmap tracks, not side notes.

## 4. Product north star

synrepo v1 succeeds when an agent on a fresh clone of an unfamiliar repository can orient quickly and begin useful work using structural cards, without depending on commentary or speculative linking.

Everything in this roadmap should improve one or more of these outcomes:

- fewer blind reads
- fewer wrong-file edits
- lower token burn
- faster orientation
- strong trust separation between graph and overlay
- low-ceremony onboarding for vibe coders
- enough review surfaces for disciplined teams

## 5. Roadmap tracks

## Track A — Foundation and project scaffolding

### Goal

Create the planning structure, guardrails, and repo conventions before implementation spreads.

### Scope

- adopt OpenSpec folder layout
- define the enduring domain specs
- sharpen contract ownership where code and docs already outrun the initial spec spine
- define the initial change taxonomy
- define naming rules for roadmap items and changes
- define what belongs in specs versus runtime docs versus generated exports

### Deliverables

- `ROADMAP.md`
- `openspec/specs/` durable spine
- `openspec/config.yaml`
- conventions for `openspec/changes/<change-name>/`
- a short contributor workflow for proposing and landing changes

### Exit criteria

- roadmap accepted
- core domain specs created
- weak contract areas tightened before implementation spreads
- first implementation changes can be opened without arguing about structure

---

## Track B — Bootstrap UX

### Goal

Reduce setup friction so a user can install synrepo, point it at a repo, and get useful orientation quickly.

### Why this belongs early

The default user will not maintain a documentation garden. Bootstrap UX is part of the wedge, not polish.

### Scope

- `synrepo init`
- repo discovery and mode selection
- first-run setup with sensible defaults
- generated agent shims and minimal instructions where useful
- first-run status and “what to do next”
- project health checks after init
- refresh and re-entry behavior after repository shape changes or prior partial setup

### Deliverables

- initialization CLI
- auto versus curated mode selection
- install-time detection of docs directories and rationale sources
- generated thin agent guidance files where appropriate
- first-run overview command
- explicit health states and bootstrap re-entry rules

### Exit criteria

- fresh user reaches first useful output in one short flow
- no mandatory manual authoring before structural value appears

---

## Track C — Lexical substrate

### Goal

Establish deterministic search and text indexing as the base layer.

### Scope

- syntext integration
- corpus discovery
- file classification
- encoding handling
- ignore rules
- indexing lifecycle and compaction boundaries
- language adapter support policy and grammar maintenance rules

### Deliverables

- file walker and filters
- text indexing
- exact search API
- index lifecycle policies
- basic CLI search
- language support criteria and grammar validation expectations

### Exit criteria

- fast and correct lexical retrieval on ugly repos
- storage layout stable enough for the graph layer to build on

---

## Track D — Structural graph

### Goal

Build the observed-facts core.

### Scope

- tree-sitter parsing
- markdown/frontmatter/link parsing
- git mining
- graph schema
- provenance and epistemic labels
- rename, split, and merge identity handling
- drift scoring
- minimum graph provenance requirements

### Deliverables

- graph store
- parsers and adapters
- observed edge extraction
- trust and conflict primitives
- structural pipeline
- auditable graph provenance contract

### Exit criteria

- the graph updates continuously from repo state
- graph facts are clearly separated from inferred overlay content
- identity instability is handled well enough for normal refactors

---

## Track E — Cards and MCP surface

### Goal

Ship the first full product milestone.

### Scope

- structural card compiler
- token budget protocol
- task-first MCP tools
- core card types
- freshness labeling
- routing guidance for agents

### Deliverables

- `SymbolCard`
- `FileCard`
- `ModuleCard`
- `EntryPointCard`
- `CallPathCard`
- `ChangeRiskCard`
- `PublicAPICard`
- `TestSurfaceCard`
- MCP server and core tools
- card budget enforcement

### Exit criteria

- Phase 2 usability target is met
- agents can answer “where should I edit?” and related questions from cards alone

---

## Track F — Pattern surface and rationale support

### Goal

Add human-declared guidance without making prose the primary product.

### Scope

- optional pattern documents
- ADR and decision ingestion
- inline rationale markers
- DecisionCards
- policy for promoted patterns versus generated commentary

### Deliverables

- pattern format and location
- rationale extraction rules
- links between patterns, decisions, and cards
- DecisionCard support
- clear rules for curated mode promotion

### Exit criteria

- patterns enrich cards and routing, but do not replace structural truth
- rationale survives in a form useful to both auto and curated users

---

## Track G — Targeted repair loop

### Goal

Borrow the best operational idea from mex: fix only what drifted.

### Scope

- check current state versus declared intent
- identify stale exports, stale docs, stale overlay entries, broken links, and trust conflicts
- repair selected surfaces without re-synthesizing everything
- make this usable from CLI and CI

### Deliverables

- `synrepo check`
- `synrepo sync`
- drift categories
- targeted repair plans
- resolution logging
- selective refresh behavior

### Exit criteria

- users can repair stale surfaces cheaply
- repair does not collapse graph and overlay into one trust bucket

---

## Track H — Watcher, reconcile, and daemon operations

### Goal

Keep the system current under real developer churn.

### Scope

- file watching
- event coalescing
- reconcile pass
- daemon lifecycle
- locking model
- cache lifecycle
- failure recovery
- retention, rebuild, and migration operations for runtime stores

### Deliverables

- watch mode
- periodic reconcile pass
- local socket or process model
- operational diagnostics
- compact and cleanup commands
- storage maintenance and migration behavior

### Exit criteria

- watcher misses do not silently poison the system
- the daemon remains optional where possible but reliable when used

---

## Track I — Git intelligence

### Goal

Improve change impact and routing with repository history.

### Scope

- ownership
- hotspots
- co-change
- last meaningful change
- churn-aware ranking
- trust-aware conflict handling with rationale sources
- degraded-history behavior for shallow or partial repositories

### Deliverables

- git-derived enrichments in cards
- impact and blast-radius improvements
- better “where to edit” ranking
- history-aware change risk scoring
- explicit `git_observed` contract for history-derived fields

### Exit criteria

- git signals materially improve routing quality without becoming canonical truth

---

## Track J — Commentary overlay

### Goal

Add commentary only after the structural product works.

### Scope

- commentary generation on demand
- freshness tagging
- cache and cost controls
- opt-in blocking freshness for high-stakes use
- source-store labeling in responses
- minimum provenance and audit fields for overlay artifacts

### Deliverables

- commentary overlay tables
- freshness and staleness model
- budget accounting
- background refresh behavior

### Exit criteria

- commentary is useful but clearly secondary
- agents can distinguish graph-backed truth from commentary

---

## Track K — Evidence-verified cross-links

### Goal

Enable bounded semantic linking without contaminating the graph.

### Scope

- candidate generation
- two-stage triage
- fuzzy evidence verification
- confidence scoring
- link-type allowlists
- findings and contradiction detection

### Deliverables

- proposed link pipeline
- verification engine
- overlay link store
- contradiction reports
- review and promotion surfaces for curated mode

### Exit criteria

- cross-links are auditable, non-authoritative, and operationally affordable

---

## Track L — Exports, ecosystem, and polish

### Goal

Provide convenience surfaces without changing the trust model.

### Scope

- export views and generated docs as convenience surfaces
- thin agent shims and onboarding polish
- more languages and file classes
- packaging, upgrade, schema migration, and maintenance flows

Ownership map:

- exports and generated views belong to the exports/views spec and participate in repair-loop freshness rules
- thin agent shims and onboarding polish belong to bootstrap
- more languages and file classes belong to substrate
- upgrade, schema migration, retention, and maintenance flows belong to storage/compatibility and watch-and-ops

### Deliverables

- generated export files
- upgrade/update flow
- schema migrations
- additional grammar support
- polish around onboarding and operations

### Exit criteria

- convenience layers stay subordinate to graph truth and card delivery

## 6. Recommended milestone order

This order governs implementation and milestone completion.

For planning, later-milestone OpenSpec changes may be opened early when their purpose is to sharpen durable contracts that earlier implementation work depends on. Opening a change early does not mean implementing it early. Execution should still follow the milestone order below unless the roadmap is explicitly revised.

### Milestone 0 — Foundation setup

Tracks:
- A Foundation and project scaffolding

Primary outcome:
- stable planning structure

Status:
- Complete through `foundation-bootstrap`

### Milestone 1 — First-run value

Tracks:
- B Bootstrap UX
- C Lexical substrate

Primary outcome:
- initialization plus deterministic search

Status:
- Complete through `lexical-substrate-v1`, `bootstrap-ux-v1`, and `storage-compatibility-v1`

### Milestone 2 — Observed-facts core

Tracks:
- D Structural graph
- H Watcher, reconcile, and daemon operations
- instability handling from the start

Primary outcome:
- continuously updated graph with stable-enough identities

Status:
- In progress, with `structural-graph-v1` complete
- `structural-pipeline-v1` is the next implementation change for automatic graph population
- `watch-reconcile-v1` is planning-ready and follows after `structural-pipeline-v1`

### Milestone 3 — First real product release

Tracks:
- E Cards and MCP surface
- I Git intelligence

Primary outcome:
- phase-2 ship target reached without LLM synthesis

### Milestone 4 — Human-guidance enrichment

Tracks:
- F Pattern surface and rationale support
- G Targeted repair loop

Primary outcome:
- human-declared guidance and cheap drift repair

### Milestone 5 — Optional intelligence layers

Tracks:
- J Commentary overlay
- K Evidence-verified cross-links

Primary outcome:
- bounded machine assistance on top of a trustworthy core

### Milestone 6 — Expansion and hardening

Tracks:
- L Exports, ecosystem, and polish

Primary outcome:
- broader adoption, more integrations, stronger maintenance story

## 7. OpenSpec domain specs to maintain now

These are the durable specs that should exist and stay sharp in `openspec/specs/` before major implementation spreads.

## 7.1 `openspec/specs/foundation/spec.md`

Purpose:
- define mission, product wedge, target users, modes, and non-goals

Must cover:
- context compiler positioning
- auto mode versus curated mode
- graph versus overlay separation
- cards as the product
- anti-goals such as “not a chunk-soup RAG layer”

Ties to roadmap:
- Track A
- governs all later tracks

## 7.2 `openspec/specs/substrate/spec.md`

Purpose:
- define lexical indexing behavior and file handling contract

Must cover:
- discovery rules
- ignore rules
- encoding behavior
- supported text classes
- compaction boundaries
- search guarantees

Ties to roadmap:
- Track C

## 7.3 `openspec/specs/storage-and-compatibility/spec.md`

Purpose:
- define `.synrepo/` storage layout responsibilities, compatibility-sensitive config, migration rules, and rebuild behavior

Must cover:
- canonical versus disposable stores
- per-store compatibility actions (`continue`, `rebuild`, `invalidate`, `clear-and-recreate`, `migrate-required`, `block`)
- retention and compaction boundaries
- schema migration versus rebuild policy
- compatibility-sensitive config fields grouped by indexing, graph, history, and operational effects
- upgrade and maintenance flows

Ties to roadmap:
- Track H
- Track L

## 7.4 `openspec/specs/graph/spec.md`

Purpose:
- define canonical node, edge, provenance, epistemic label, and drift behavior

Must cover:
- file, symbol, and concept nodes
- allowed edge types
- trust labels
- conflict rules
- rename, split, and merge semantics

Ties to roadmap:
- Track D
- Track I
- instability handling

## 7.5 `openspec/specs/cards/spec.md`

Purpose:
- define card contracts and budget tiers

Must cover:
- card types
- required fields
- tier behavior
- truncation priority
- freshness labeling
- graph-backed versus overlay-backed fields

Ties to roadmap:
- Track E
- Track J

## 7.6 `openspec/specs/mcp-surface/spec.md`

Purpose:
- define task-first tools and response contracts

Must cover:
- overview
- card lookup
- where-to-edit
- change impact
- entrypoints
- call path
- test surface
- minimum context
- findings
- provenance
- freshness flags

Ties to roadmap:
- Track E

## 7.7 `openspec/specs/bootstrap/spec.md`

Purpose:
- define first-run UX, project initialization, and generated assistant-facing setup

Must cover:
- `synrepo init`
- mode choice
- repo inspection
- generated shims
- health checks
- update and refresh behavior
- idempotence and re-entry behavior
- mandatory first-run outputs

Ties to roadmap:
- Track B
- Track L

## 7.8 `openspec/specs/patterns-and-rationale/spec.md`

Purpose:
- define the optional human-guidance layer

Must cover:
- pattern file format
- ADR and decision ingestion
- inline rationale markers
- promotion rules
- DecisionCard behavior

Ties to roadmap:
- Track F

## 7.9 `openspec/specs/repair-loop/spec.md`

Purpose:
- define targeted drift detection and repair workflow

Must cover:
- drift classes
- stale exports
- stale overlay
- broken declared links
- stale rationale
- selective sync behavior
- CI behavior

Ties to roadmap:
- Track G

## 7.10 `openspec/specs/watch-and-ops/spec.md`

Purpose:
- define watcher, reconcile, locking, daemon, cache, compact, retention, and operational diagnostics behavior

Must cover:
- event coalescing
- reconcile intervals
- failure recovery
- operational status
- retention
- compaction
- single-writer safety
- migration and rebuild operations

Ties to roadmap:
- Track H

## 7.11 `openspec/specs/git-intelligence/spec.md`

Purpose:
- define git-derived routing, ranking, and change-risk enrichment behavior

Must cover:
- ownership
- hotspots
- co-change
- last meaningful change
- churn-aware ranking
- degraded-history behavior
- `git_observed` authority boundaries

Ties to roadmap:
- Track I

## 7.12 `openspec/specs/overlay/spec.md`

Purpose:
- define machine-authored commentary and proposed-link behavior

Must cover:
- commentary contract
- freshness states
- source-store labels
- cost limits
- review surfaces
- overlay never overrides graph
- minimum overlay provenance fields
- audit trail exposure rules

Ties to roadmap:
- Track J
- Track K

## 7.13 `openspec/specs/exports-and-views/spec.md`

Purpose:
- define generated exports and runtime views as convenience surfaces subordinate to graph truth

Must cover:
- runtime views versus exports
- freshness and stale markers
- repair-loop participation
- auto versus curated mode applicability
- generated outputs never becoming synthesis input by default

Ties to roadmap:
- Track G
- Track L

## 7.14 `openspec/specs/evaluation/spec.md`

Purpose:
- define success metrics, anti-metrics, and benchmark conditions

Must cover:
- time-to-first-useful-result
- task success delta
- token savings
- wrong-file edit rate
- overlay reliance in high-stakes actions
- contradiction rate
- budget escalation rate
- ugly-repo benchmark expectations

Ties to roadmap:
- all tracks, especially release gates

## 8. Initial change set to create under `openspec/changes/`

These should be the first concrete OpenSpec changes opened against the foundation specs.

The list below is the default implementation sequence, not a ban on opening a later change early for contract-definition work. If a later-milestone change is opened early, its proposal and design should make clear that it exists to sharpen planning boundaries ahead of implementation and does not reorder milestone execution by itself.

## 8.1 `openspec/changes/foundation-bootstrap/`

Use for:
- creating project config, enduring specs, conventions, and contributor workflow

Artifacts:
- proposal
- design
- tasks
- delta specs touching `foundation`, `bootstrap`, and `evaluation`

Roadmap tie:
- Milestone 0

## 8.2 `openspec/changes/lexical-substrate-v1/`

Use for:
- syntext integration, corpus discovery, file handling, indexing, search CLI, and language adapter support rules

Roadmap tie:
- Milestone 1

## 8.3 `openspec/changes/bootstrap-ux-v1/`

Use for:
- init flow, mode selection, health checks, generated shims, first-run overview, and re-entry semantics

Roadmap tie:
- Milestone 1

## 8.4 `openspec/changes/archive/2026-04-10-structural-graph-v1/`

Use for:
- parsers, graph schema, provenance, trust labels, drift, and identity handling

Roadmap tie:
- Milestone 2

## 8.5 `openspec/changes/structural-pipeline-v1/`

Use for:
- structural compile producers, automatic graph population, bootstrap-triggered graph refresh, and deterministic produced-slice replacement

Roadmap tie:
- Milestone 2

## 8.6 `openspec/changes/watch-reconcile-v1/`

Use for:
- watcher, reconcile pass, locking, runtime status, compaction basics, and store maintenance operations

Planning note:
- sequence this after `structural-pipeline-v1`, so watch and reconcile rerun a real structural producer path instead of inventing a second source of graph truth

Roadmap tie:
- Milestone 2

## 8.7 `openspec/changes/cards-and-mcp-v1/`

Use for:
- card compiler, budgets, MCP tools, and first product usability target

Roadmap tie:
- Milestone 3

## 8.8 `openspec/changes/git-intelligence-v1/`

Use for:
- co-change, hotspots, ownership, and improved change impact ranking

Planning note:
- may be opened early for contract sharpening once the durable spec spine exists, but implementation still follows Milestone 3 after Milestone 2 observed-facts work is in place

Roadmap tie:
- Milestone 3

## 8.9 `openspec/changes/pattern-surface-v1/`

Use for:
- patterns, rationale ingestion, DecisionCards, and curated promotion rules

Roadmap tie:
- Milestone 4

## 8.10 `openspec/changes/repair-loop-v1/`

Use for:
- `check`, `sync`, drift classification, selective refresh, and resolution logging

Roadmap tie:
- Milestone 4

## 8.11 `openspec/changes/commentary-overlay-v1/`

Use for:
- commentary generation, freshness controls, cost accounting, cache behavior, and overlay provenance

Roadmap tie:
- Milestone 5

## 8.12 `openspec/changes/cross-link-overlay-v1/`

Use for:
- candidate generation, verification, confidence scoring, findings, and review flows

Roadmap tie:
- Milestone 5

## 8.13 `openspec/changes/export-and-polish-v1/`

Use for:
- generated docs and views, tool shims, update flows, extra file support, packaging polish, and export freshness/repair rules

Roadmap tie:
- Milestone 6

## 8.14 `openspec/changes/storage-compatibility-v1/`

Use for:
- `.synrepo/` store classes, compatibility-sensitive config, rebuild versus migration policy, thin runtime compatibility metadata, and maintenance semantics shared by storage and ops

Planning note:
- this is the active Milestone 1 hardening slice because bootstrap already owns `.synrepo/` layout and substrate already persists index state
- implement the contract-first runtime layer now, but keep the scope narrow: compatibility metadata, store checks, and clear CLI guidance, not full migration or daemon work

Roadmap tie:
- Track H
- Track L

## 9. What not to do

To keep the roadmap aligned with the product thesis, avoid the following:

1. Do not make generated markdown the primary interface.
2. Do not let OpenSpec specs become a substitute for graph-backed runtime truth.
3. Do not make pattern files mandatory for value.
4. Do not let commentary or proposed links enter the graph directly.
5. Do not postpone instability handling until late phases.
6. Do not ship “AI understanding” before Phase 2 cards work on code-only repos.
7. Do not treat watch, reconcile, and repair as polish. They are trust features.

## 10. Release gating guidance

A milestone should not be marked complete unless its behavior is captured in the relevant domain spec and its change artifacts are archived cleanly.

Use this rule:

- domain specs describe stable intended behavior
- changes describe what is being altered right now
- the graph describes what the repo currently contains
- the overlay describes machine-authored supplements
- exports are convenience surfaces only

## 11. Suggested next move

`lexical-substrate-v1`, `bootstrap-ux-v1`, `storage-compatibility-v1`, and `structural-graph-v1` are complete. The next practical steps are:

1. implement `structural-pipeline-v1` to make graph population automatic from repository state
2. implement `watch-reconcile-v1` after that producer path lands, so watcher and reconcile behavior drive a real structural refresh loop
3. keep `git-intelligence-v1` planning-ready until the observed-facts update path is stable enough to support history-derived evidence

Keep `git-intelligence-v1` in planning-ready state through Milestone 2. Its contract is stable; implementation depends on the graph layer being in place first.

This order continues Milestone 2 now that the storage contract and first direct graph inspection surface are in place.
