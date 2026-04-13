# synrepo — Product Spec

A context compiler for AI coding agents.

## 1. Purpose

synrepo exists to reduce context pressure for AI coding agents.

> **Implementation status (2026-04-12):** Stages 1–5 of the structural pipeline are
> shipped, including cross-file `calls` and `imports`, file-scoped Git intelligence,
> content-hash rename reuse, commentary freshness, and the optional watch runtime.
> `synrepo watch`, `synrepo watch --daemon`, `synrepo watch status`, `synrepo watch stop`,
> `synrepo status`, `synrepo check`, `synrepo sync`, and `synrepo agent-setup` are
> shipped. The stdio MCP server is shipped with the core task-first tools. Remaining
> follow-on work includes specialist cards and MCP tools, graph-level drift scoring,
> and broader overlay workflows.

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

> **Current state:** `GraphCardCompiler` serves `SymbolCard`, `FileCard`, and
> `DecisionCard` today. `ModuleCard` exists as a struct shape only. Entry-point,
> call-path, change-risk, public-API, and test-surface cards remain follow-on work.

### Budget tiers

All task-shaped MCP tools return cards with an explicit budget tier.

* **tiny**: default, orientation-first
* **normal**: fuller local understanding
* **deep**: includes source bodies and optional commentary

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

> **Current state:** commentary overlay storage and freshness labeling are shipped,
> with the graph and overlay still physically separated. Broader machine-authored
> link workflows remain additive and should not be treated as graph truth.

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

### 9.2 Synthesis pipeline *(Phase 4+ — not yet implemented)*

> **Current state:** `src/pipeline/synthesis.rs` is a 4-line stub. No LLM call
> is made anywhere in the codebase today.

Cold path. LLM-driven. Optional.

Produces:

* commentary for cards
* proposed cross-links
* findings

Never blocks the structural pipeline. Never changes the graph directly.

## 10. Cross-linking

Cross-linking is an overlay feature, not a graph feature.

It exists to help an agent connect code and prose across vocabulary gaps, but it is not required for the core product.

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

> **Current state:** the stdio MCP server is shipped. Today it exposes
> `synrepo_overview`, `synrepo_card`, `synrepo_search`, `synrepo_where_to_edit`,
> `synrepo_change_impact`, and `synrepo_findings`. The specialist tools below
> remain planned follow-on surface area.

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

### Phase 0 — lexical substrate

* syntext integration
* discovery and indexing
* robust file handling

### Phase 1 — structural graph

* tree-sitter parsing
* markdown parsing
* git mining
* drift scoring
* sqlite graph

### Phase 2 — cards + MCP, no LLM

This is the first full product milestone.

Deliver:

* structural card compiler
* token budget protocol
* task-first MCP tools
* useful orientation in under 60 seconds on a 10k-file unfamiliar repo

### Phase 3 — git intelligence and rationale support

* hotspots
* ownership
* co-change enrichments
* optional DecisionCards where human rationale exists

### Phase 4 — commentary overlay

* on-demand commentary for cards
* freshness controls
* caching and cost accounting

### Phase 5 — cross-link overlay

* hybrid candidate generation
* evidence-verified proposed links
* findings and contradiction detection

### Phase 6 — polish and expansion

* compaction and retention polish
* optional export views
* more languages
* more file types

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
