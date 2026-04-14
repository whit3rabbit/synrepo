# synrepo — Product Spec

A context compiler for AI coding agents.

## 1. Purpose

synrepo exists to reduce context pressure for AI coding agents.

> **Implementation status (2026-04-13):**
>
> *Shipped.* Structural pipeline stages 1 through 5 (discovery, code parse across
> Rust / Python / TypeScript / Go, prose parse, cross-file `calls` and `imports`
> resolution, file-scoped Git intelligence). Stage 6 ships content-hash rename
> detection (`path_history` preservation on moves). Compiled card set: `SymbolCard`,
> `FileCard`, `ModuleCard`, `EntryPointCard`, `DecisionCard`. The stdio MCP server
> exposes eight tools: `synrepo_overview`, `synrepo_card`, `synrepo_module_card`,
> `synrepo_search`, `synrepo_where_to_edit`, `synrepo_change_impact`,
> `synrepo_entrypoints`, `synrepo_findings`. Cross-link overlay shipped
> (candidate triage, opt-in Claude generator, review queue, card surfacing at
> Deep tier). CLI shipped: `init`, `reconcile`, `check`, `sync`, `status`,
> `export`, `upgrade`, `agent-setup` (claude / cursor / copilot / generic / codex /
> windsurf), `watch` (foreground / daemon / status / stop), `search`, `graph`,
> `node`.
>
> *Improvements over earlier plan (see §§5, 7.3, 9.2, 10, 12, 17 for rationale).*
> Graph and overlay each persist to a single SQLite file (`nodes.db`, `overlay.db`)
> instead of split per-edge-type stores; cross-link candidate generation uses
> graph-distance plus prose-identifier triage and defers embeddings (`ort` /
> MiniLM) until a repo size proves them necessary; the synthesis layer is trait-
> shaped (`CommentaryGenerator`, `CrossLinkGenerator`) with `NoOp` defaults so
> the product still ships LLM-free while preserving a clean opt-in path.
>
> *Known laziness drift (not improvements, planned to close).* `FileCard` returns
> `git_intelligence: None` at `src/surface/card/compiler/file.rs:81` even though
> Stage 5 populates the data; the `git-data-surfacing-v1` change closes this
> wiring gap. `SymbolCard.last_change` is unimplemented (design captured in the
> same change). Graph-level `CoChangesWith` edges are not yet emitted even
> though file-scoped co-change is computed.
>
> *Remaining follow-on.* Stage 6 split / merge detection; Stage 7 drift scoring;
> Stage 8 ArcSwap commit; `CallPathCard`, `ChangeRiskCard`, `PublicAPICard`,
> `TestSurfaceCard` plus their matching MCP tools; low-level MCP primitives
> (`synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`,
> `synrepo_provenance`); optional Phase 5 embedding-based candidate hybrid.

It precomputes a small, deterministic, queryable working set about a repository and serves it through MCP in token-budgeted packets called **cards**. The goal is not to build a browsable ontology or a generated wiki. The goal is to help an agent answer questions like:

* Where should I edit?
* What breaks if I change this?
* What is the minimum file set for this task?
* What tests constrain this behavior?
* Where does execution enter this subsystem?

The product wedge is concrete:

* fewer blind reads
* fewer wrong-file edits
* lower token burn
* faster orientation on unfamiliar code

The graph is infrastructure. Cards are the product.

## 2. Primary users

### Auto mode

The default user is the vibe coder working with an AI coding agent. They will not curate ontology, review findings regularly, or maintain documentation just to make synrepo useful. synrepo must deliver value with near-zero ceremony.

### Curated mode

The secondary user is a disciplined team that already maintains ADRs, design docs, or inline rationale markers. They want stronger review workflows and more durable rationale links.

Both modes share the same architecture. The difference is how much review surface is exposed and whether overlay proposals are turned into human-authored declarations.

## 3. Core design rules

1. **Observed facts only in the graph.**
   The canonical graph stores only parser-observed, git-observed, and human-declared facts.

2. **Machine-authored content lives in an overlay.**
   LLM commentary, proposed cross-links, and findings are stored separately from the graph.

3. **The synthesis pipeline never reads overlay output as input.**
   This is a hard contamination rule enforced by storage separation and retrieval filtering.

4. **Cards default to tiny.**
   The system should return the smallest truthful answer first and let the agent escalate.

5. **Phase 2 is the product.**
   synrepo must be useful before any LLM call exists.

## 4. Non-goals

synrepo is not:

* a wiki generator as the primary product
* a vector database with semantic search as the source of truth
* a generic RAG layer that returns chunk soup
* an ontology engine for auto-minting concepts from code
* a documentation platform that expects humans to browse generated pages
* a generic session-memory or cross-session agent-memory product
* a hook-driven auto-capture system that records what callers did
* a background "magic" service with invisible ownership (watch is explicit, per-repo, and stoppable)
* a vector-first retrieval system (embeddings remain a bounded, opt-in candidate generator for overlay cross-links only)

Semantic embeddings are allowed only as a bounded candidate generator for optional overlay cross-linking.

## 5. The user-facing abstraction: cards

A **card** is a small, structured, deterministic record compiled from the live graph and source state.

Cards are not summaries. A summary is prose. A card is a structured fact packet with a token budget.

### Core card types in v1

* **SymbolCard**: what a symbol is, where it lives, who calls it, what it calls, which tests touch it
* **FileCard**: what is in a file, what depends on it, recent meaningful changes
* **ModuleCard**: public surface and role of a directory or package
* **EntryPointCard**: how execution enters a subsystem
* **CallPathCard**: shortest relevant control-flow path between A and B
* **ChangeRiskCard**: likely blast radius of a change
* **PublicAPICard**: externally visible surface
* **TestSurfaceCard**: tests and assertions constraining behavior
* **DecisionCard**: optional rationale card when human-authored decision material exists

> **Current state:** `GraphCardCompiler` compiles `SymbolCard`, `FileCard`,
> `ModuleCard`, `EntryPointCard`, and `DecisionCard`.
>
> *Improvement vs earlier plan.* `EntryPointCard` (binary / cli_command /
> http_handler / lib_root classification) and `ModuleCard` (directory
> aggregation: files, nested modules, public symbols) both shipped in
> `cards-and-mcp-v1`, ahead of the prior "struct shape only" status note.
>
> *Laziness drift.* `FileCard.git_intelligence` is hardcoded `None` at
> `src/surface/card/compiler/file.rs:81` despite Stage 5 already computing
> `GitPathHistoryInsights`. The `From<GitPathHistoryInsights>` conversion in
> `src/surface/card/git.rs` exists but is not wired. This is wiring work, not
> a design question; `git-data-surfacing-v1` closes it. `SymbolCard.last_change`
> is unimplemented for the same reason (see §17 phase plan).
>
> *Remaining follow-on.* `CallPathCard`, `ChangeRiskCard`, `PublicAPICard`,
> `TestSurfaceCard`.

### Budget tiers

All task-shaped MCP tools return cards with an explicit budget tier. The tiers form a three-surface progressive-disclosure protocol — an intentional interaction pattern, not an internal truncation knob. Agents are expected to escalate deliberately (orient, then understand locally, then fetch deeply), the same shape as good library search APIs.

* **tiny**: default, orientation-first — the *index surface*
* **normal**: fuller local understanding — the *neighborhood surface*
* **deep**: includes source bodies and optional commentary — the *fetch-on-demand surface*

Budgets are enforced server-side. Cards are truncated by priority, not by accident.

### Card freshness

Structural card fields are always sourced from the graph and current source state. They are as fresh as the last structural compile.

Optional commentary attached to a card comes from the overlay and is labeled `fresh`, `stale`, or `missing`.

## 6. Product acceptance target

synrepo v1 succeeds if:

> An agent on a fresh clone of an unfamiliar 10k-file repository can orient and begin producing useful code in under 60 seconds using Phase 2 cards, without any LLM synthesis.

Everything beyond that is additive.

## 7. Architecture

synrepo has four layers.

### 7.1 Substrate layer

* `syntext` provides deterministic lexical indexing and exact lookup.
* It is used for naming, search fallback, and citation verification.

### 7.2 Structure layer

The structure layer builds the canonical graph from directly observed facts.

Sources:

* tree-sitter for code structure
* markdown/frontmatter/link parsing for prose structure
* git mining for change history, co-change, ownership, rename hints

Store:

* sqlite in v1
* single source of truth
* no in-memory graph mirror unless benchmarks prove it is needed

### 7.3 Overlay layer

> **Current state:** commentary overlay and cross-link overlay are both shipped
> (`cross-link-overlay-v1`, now archived). Overlay stores commentary with
> content-hash freshness labels and cross-link proposals with cited spans,
> confidence tiers, and an explicit review queue. The synthesis pipeline never
> reads overlay content as input.
>
> *Improvement vs earlier plan.* Graph (`.synrepo/graph/nodes.db`) and overlay
> (`.synrepo/overlay/overlay.db`) each live in a single SQLite file rather than
> the split `nodes.db` / `edges.db` / `provenance.db` and split
> `cross_links.db` / `commentary/` layout sketched in v4 of FOUNDATION.md. One
> file per store keeps every multi-table read in one atomic snapshot and lets
> the compatibility check version each store as a single unit. The stores
> remain physically separated from each other (the contamination invariant is
> unaffected).
>
> *Curated-mode promotion.* Accepting a cross-link in curated mode creates a
> `Governs` edge with `Epistemic::HumanDeclared`; the overlay record stays in
> the audit trail.

The overlay stores machine-authored outputs:

* card commentary
* proposed cross-links
* findings and inconsistencies

The overlay is queryable by MCP but never treated as canonical and never read by synthesis as input.

### 7.4 Surface layer

The surface layer is:

* CLI for local operation and CI
* MCP server for agent access
* a thin skill/instructions bundle for agent behavior

## 8. Data model

### Graph nodes

Only three kinds in v1:

* **File**
* **Symbol**
* **Concept** only when backed by a human-authored concept or ADR file

No machine-authored concept nodes exist in the graph.

### Graph edges

Only observed or declared edge types belong in the graph. Examples:

* `defines`
* `calls`
* `imports`
* `inherits`
* `references`
* `mentions`
* `co_changes_with`
* `governs` only when explicitly declared by human-authored metadata or inline markers

Every graph row carries:

* provenance
* epistemic status (`parser_observed`, `git_observed`, `human_declared`)
* drift score where applicable

### Overlay entries

Overlay entries may refer to graph nodes but do not become graph facts.

Types:

* **commentary**
* **proposed_link**
* **finding**

Epistemic statuses:

* `machine_authored_high_conf`
* `machine_authored_low_conf`

Curated mode may allow a human to convert an overlay proposal into a human-authored declaration. That conversion creates new human-authored source material first. The graph is updated only from that declared source, never directly from LLM output.

## 9. Two pipelines

### 9.1 Structural pipeline

Hot path. No LLM.

Runs on file changes and produces:

* parsed symbol graph
* file and module relationships
* git-derived signals
* drift scores
* card-ready structural facts

This pipeline must remain fast, deterministic, and cheap.

### 9.2 Synthesis pipeline *(Phase 4+)*

> **Current state:** `src/pipeline/synthesis/` ships a trait-shaped boundary with
> two traits and four implementations. `CommentaryGenerator` has `NoOpGenerator`
> (default, returns `Ok(None)`) and `ClaudeCommentaryGenerator` (calls the
> Claude Messages API when `SYNREPO_ANTHROPIC_API_KEY` is set).
> `CrossLinkGenerator` has `NoOpCrossLinkGenerator` and a Claude-backed
> generator used by `synrepo sync --generate-cross-links`. Commentary is
> invoked lazily by the card compiler at `Deep` budget when no fresh overlay
> entry exists; cross-link generation runs only via the explicit sync command.
>
> *Improvement vs earlier plan.* The earlier doc listed synthesis as "not yet
> implemented". Reality: the trait boundary exists from day one so the default
> install is LLM-free and deterministic while a key-gated opt-in path produces
> overlay content without any graph coupling. No ambient LLM call path exists.

Cold path. LLM-driven. Optional.

Produces:

* commentary for cards
* proposed cross-links
* findings

Never blocks the structural pipeline. Never changes the graph directly.

## 10. Cross-linking

Cross-linking is an overlay feature, not a graph feature.

It exists to help an agent connect code and prose across vocabulary gaps, but it is not required for the core product.

> **Current implementation.** Candidate generation is graph-distance plus
> prose-identifier triage (see `src/pipeline/synthesis/cross_link/triage.rs`
> and `candidate_pairs`). Embeddings (`ort` / `tokenizers` for
> `all-MiniLM-L6-v2`) are deferred: the dependencies stay commented in
> `Cargo.toml` with a Phase 5 marker. This is a conscious deferral rather than
> a gap. Graph proximity bounds the work by repo topology instead of by 384-
> dimensional vector math, avoids the embedding model cache / churn problem,
> and keeps the Phase 2 product deterministic. Embeddings can be added later
> as an optional hybrid component once a repo size shows that name-based
> triage misses too many true candidates. The four-stage contract below
> (candidate generation, typed-proposal, verification, limit) is otherwise
> unchanged.

### Candidate generation

Hybrid scoring:

* semantic similarity
* graph proximity
* directory locality

### Proposal

The LLM may propose a typed relationship only when it can cite supporting spans.

### Verification

Verification is normalized and fuzzy, not byte-exact:

* text normalization
* token-level fuzzy comparison
* snapping to the actual source span
* node existence checks
* link-type allowlists

### Important limit

Verified citations prove the cited text exists. They do not prove the semantic inference is correct. That is why cross-links remain overlay content.

## 11. Trust model

synrepo answers two different kinds of questions and should rank evidence differently for each.

### Descriptive trust: what does the code do now?

Highest to lowest:

1. code structure
2. tests
3. inline human declarations close to code
4. git-observed patterns
5. prose documents
6. overlay

### Normative trust: why was this built this way?

Highest to lowest:

1. inline human rationale markers
2. ADRs and decision docs
3. design docs and README material
4. commit messages
5. code
6. tests
7. overlay

### Conflict rules

* Overlay never overrides graph.
* Code can contradict intent documents; both may still matter for different questions.
* Two human sources in direct conflict are surfaced as a finding, not silently resolved.

## 12. MCP surface

> **Current state:** the stdio MCP server ships eight tools today:
> `synrepo_overview`, `synrepo_card`, `synrepo_module_card`, `synrepo_search`,
> `synrepo_where_to_edit`, `synrepo_change_impact`, `synrepo_entrypoints`,
> `synrepo_findings`.
>
> *Improvement vs earlier plan.* `synrepo_entrypoints` and `synrepo_module_card`
> both shipped ahead of the prior "planned follow-on" status. `synrepo_module_card`
> is a dedicated directory-targeted tool matching the "module tool should stand
> alone but be enhanced by MCP" design resolution.
>
> *Partial.* Extending `synrepo_card` to also accept a directory path (so a
> single tool accepts files, symbols, concepts, or directories) is the
> remaining half of that resolution. Not yet implemented; follows
> `synrepo_module_card` naturally once the card compiler's target resolver
> grows a directory case.
>
> *Remaining follow-on.* Task-first tools `synrepo_call_path`,
> `synrepo_test_surface`, `synrepo_minimum_context`, `synrepo_explain`,
> and `synrepo_recent_activity` (bounded lane over synrepo's own
> operational history: recent reconciles, repair-log entries, cross-link
> accept/reject, commentary refreshes, churn-hot files — not session
> memory, not agent-interaction history). No low-level primitives
> (`synrepo_node`, `synrepo_edges`, `synrepo_query`, `synrepo_overlay`,
> `synrepo_provenance`) are exposed yet; the CLI covers debugging needs
> for now.

The primary interface is task-first.

### Primary tools

* `synrepo_overview(budget?)`
* `synrepo_card(target, type?, budget?, require_freshness?)`
* `synrepo_where_to_edit(task_description, budget?)`
* `synrepo_change_impact(target, budget?)`
* `synrepo_entrypoints(scope?, budget?)`
* `synrepo_call_path(from, to, budget?)`
* `synrepo_test_surface(target, budget?)`
* `synrepo_minimum_context(task_description, budget?)`
* `synrepo_explain(target, require_freshness?)`
* `synrepo_search(query)`
* `synrepo_findings(scope?)`

### Low-level tools

* `synrepo_node(id)`
* `synrepo_edges(id, direction?, types?)`
* `synrepo_query(query)`
* `synrepo_overlay(target)`
* `synrepo_provenance(id)`

### Freshness behavior

Default is non-blocking.

* graph-backed fields are fresh
* overlay-backed fields may be stale
* the agent opts into blocking freshness only when needed with `require_freshness=true`

## 13. Identity and stability

Identity breakage is one of the biggest correctness risks.

### File identity

Primary strategy:

* AST/symbol overlap between disappeared and new files
* split and merge detection where structural evidence exists
* fallback to git rename hints
* degrade gracefully when neither helps

### Symbol identity

Symbol identity is anchored to:

* file identity
* qualified name
* kind
* body hash

Pathological refactors will still break identity. The system should surface findings and recover lazily rather than pretending to solve every case.

## 14. File and repository scope

v1 should stay narrow.

Supported strongly in v1:

* Rust
* Python
* TypeScript/TSX
* markdown-based prose

Other file types may be indexed lexically without full structure.

Out of scope in v1:

* cross-repo linking
* cross-language call graph resolution
* image understanding
* deep PDF structure extraction

## 15. Operational requirements

* single-writer model
* explicit per-repo opt-in watch mode
* local socket when watch daemon mode is running
* operation-scoped file locking via `writer.lock`
* watcher coalescing under heavy churn
* startup reconcile plus reconcile backstop to recover missed events
* compaction command for disk growth
* explicit budget caps for embeddings and LLM cache

## 16. Evaluation

### Core metrics

* time to first useful result after init
* agent task success rate with synrepo vs without
* token cost per task vs cold-context baseline
* frequency of wrong-file edits
* phase-2 usefulness without any LLM synthesis

### Behavioral metrics

* overlay reliance in high-stakes actions
* `require_freshness=true` rate before writes
* contradiction rate between overlay and later graph facts
* budget escalation rate from tiny to normal to deep

### Anti-metrics

* findings growth without review
* overlay bloat
* disk growth
* synthesis cost runaway

### Benchmark realism

Benchmark on ugly repos, not just clean demos.

## 17. Phased build plan

### Phase 0 — lexical substrate *(shipped)*

* syntext integration
* discovery and indexing
* robust file handling

### Phase 1 — structural graph *(partially shipped)*

* tree-sitter parsing (shipped: Rust, Python, TypeScript / TSX, Go)
* markdown parsing (shipped)
* git mining (shipped: history, hotspots, ownership, file-scoped co-change)
* drift scoring (Stage 7, not yet wired)
* sqlite graph (shipped: single `nodes.db`)

### Phase 2 — cards + MCP, no LLM *(shipped in part, product milestone reached)*

Delivered:

* structural card compiler (SymbolCard, FileCard, ModuleCard, EntryPointCard)
* token budget protocol (tiny / normal / deep, server-enforced)
* task-first MCP tools (8 of the planned set)
* default install is LLM-free

Remaining to fully close Phase 2:

* `CallPathCard`, `ChangeRiskCard`, `PublicAPICard`, `TestSurfaceCard`
* `synrepo_call_path`, `synrepo_test_surface`, `synrepo_minimum_context`,
  `synrepo_explain`
* `FileCard.git_intelligence` wiring (laziness drift, see §5)
* `SymbolCard.last_change`

### Phase 3 — git intelligence and rationale support *(partially shipped)*

* hotspots (shipped, not yet surfaced on cards: wiring gap)
* ownership (shipped, same wiring gap)
* co-change enrichments (file-scoped shipped; graph-level `CoChangesWith`
  edges not yet emitted)
* optional DecisionCards where human rationale exists (shipped)

### Phase 4 — commentary overlay *(shipped)*

* on-demand commentary for cards (Deep tier, lazy)
* freshness controls (content-hash; fresh / stale / missing / unsupported /
  invalid / budget_withheld)
* caching and cost accounting (overlay store; no ambient LLM call path)

### Phase 5 — cross-link overlay *(partially shipped)*

* hybrid candidate generation: graph-distance plus prose-identifier triage
  shipped; embedding-based hybrid deferred (see §10)
* evidence-verified proposed links (shipped: cited spans, normalized fuzzy
  verification, confidence tiers, review queue)
* findings and contradiction detection (`synrepo_findings`, shipped)

### Phase 6 — polish and expansion *(in progress)*

* compaction and retention polish (partial: `synrepo upgrade`, `synrepo sync`
  maintenance; dedicated `synrepo compact` not yet)
* optional export views (shipped: `synrepo export`)
* more languages (Go added since the original plan)
* more file types (pending)
* progressive-disclosure protocol doc pass (reframe `tiny/normal/deep` as a
  three-surface interaction pattern in both spec and SKILL.md copy so agents
  learn to escalate intentionally)
* `synrepo_recent_activity` MCP tool plus `synrepo status --recent` flag —
  bounded surface over `.synrepo/state/reconcile-state.json`,
  `.synrepo/state/repair-log.jsonl`, and overlay events already persisted

## 18. Unresolved risks

* wrong-but-cited inferred links
* trust drift if agents overuse overlay content
* identity instability under chaotic refactors
* embedding model churn and re-embed cost
* long-tail language maintenance cost
* operational complexity if the daemon or watcher misbehaves

## 19. Short version

synrepo should be built as a **context compiler**, not a knowledge browser.

The graph stores observed facts.
The overlay stores machine-authored suggestions.
Cards are the product.
Phase 2 is the real ship target.
Everything else is optional leverage on top.
