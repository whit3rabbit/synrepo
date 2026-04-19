# synrepo — Foundational Design (v4)

A context compiler for AI coding agents.

---

## What synrepo is

synrepo precomputes a small, deterministic, queryable working set of facts about a software project and serves that working set to coding agents through an MCP server in tight, task-shaped, token-budgeted packets called *cards*. Today the strongest shipped signals are symbol definitions, import relationships, and approximate dependency hints, with **co-change links, git hotspots, and change impact analysis** now wired into the card surface. Higher-fidelity symbol-to-symbol call graphs and cross-language dependency proof remain follow-on work.

Underneath, [syntext](https://github.com/whit3rabbit/syntext) provides deterministic lexical search; tree-sitter via per-language Rust crates provides structural parsing; sqlite holds the canonical graph of facts the parsers and git observed directly. An LLM is layered on top, strictly off the critical path, to compress long material into commentary and to propose cross-links between code and prose — but everything the LLM produces lives in a clearly separate **overlay store** that is queryable alongside the graph but is never part of it. The graph holds only what was directly observed. The overlay holds what was inferred, with provenance and confidence visible to agents that ask.

The product wedge is concrete: **fewer blind reads, fewer wrong-file edits, lower token burn, faster orientation on unfamiliar code.** The graph is infrastructure. Cards are the product. In the current implementation, that value is strongest for orientation and first-pass routing; precise cross-file impact proof is partially shipped.

---

## Who synrepo is for

**The primary user is the vibe coder** — someone who builds software primarily by directing an AI coding agent, often working solo or in a small team, often on projects whose entire codebase they did not personally write. They want their agent to understand the project as well as a senior contributor would. They will not write documentation. They will not read `findings.md`. They will not promote concept drafts via CLI. They want to install synrepo, point it at their repo, and have their coding agent immediately start producing better code with no ceremony.

**The secondary user is the disciplined team** — engineers in an organization that already invests in ADRs, design docs, and architectural rationale, and that wants a tool to keep those artifacts queryable, fresh, and connected to code. For this audience, a review surface is a feature. They will use it.

synrepo handles both with two modes selected at `synrepo init`:

| Aspect | Auto mode (default) | Curated mode |
| --- | --- | --- |
| Concept nodes | Disabled unless `docs/concepts/` or similar exists | Enabled when human-authored ADR directories are present |
| Cross-link proposals | Land in the overlay store, queryable via MCP | Same overlay, plus explicit promotion workflow |
| Bootstrap | Runs automatically; structural cards first, overlay cross-links in the background | Generates draft queue requiring explicit promotion |
| Findings | Surfaced via MCP only when asked | Also written to `findings.md` for review |
| Default card tier | `tiny` | `tiny` |

The **hard invariant** survives in both modes: the synthesis layer never reads its own previous output as retrieval input. This is enforced physically (graph and overlay are separate sqlite stores) and reinforced at the retrieval layer (synthesis queries filter on `source_store = graph`). LLM-authored content sits in the overlay where agents can see it, but the synthesis pipeline cannot. The contamination guarantee is structural, not merely labeled.

---

## Hard invariants vs current fidelity

| Topic | Hard invariant (Architecture) | Current fidelity (Implementation) |
| --- | --- | --- |
| **Separation** | Graph and overlay stores must stay physically separate. | Implemented via `nodes.db` and `overlay.db`. |
| **Doctrine** | One obvious path: `tiny` → `normal` → `deep`. | Enforced in shims, docs, and MCP descriptions. |
| **Synthesis** | LLM content is supplemental and strictly off the critical path. | Commentary is only fetched at `Deep` budget. |
| **Background** | No magic background writes without user/agent opt-in. | Commentary refresh is an explicit requested operation. |
| **Change Risk** | Signals must be derived from structural drift and co-change. | Shipped as a composite signal (beta fidelity). |
| **Fills** | Empty concept dirs must not break code orientation. | Code-only mode is the benchmark default. |

---

## Product boundaries and doctrine

Four boundary rules govern what synrepo is and how it is used. They sit above the architecture because they decide which architecture questions are even in scope.

### Product boundary: code memory, not task memory

synrepo manages code memory and bounded operational memory. It does not manage generic task memory, chat memory, or cross-session agent memory. The graph is the source of truth for observed facts. The overlay is advisory and machine-authored. Any workflow handoff surface is derived from graph and overlay state, regenerated on demand, and is not the canonical storage layer for project planning.

Concretely:

- synrepo stores what parsers, git, and humans observed about the repo.
- synrepo does not store assignments, statuses, comments, sprints, or chat logs.
- synrepo can emit structured next-action recommendations, but the authoritative task record belongs in whatever issue tracker the team already uses.

This line exists because the code-memory product and the task-memory product have different failure modes. Mixing them produces a system that is bad at both.

### Agent doctrine: one obvious way to use synrepo

There is one default agent path, and every SKILL doc, MCP tool description, `agent-setup` shim, and CLI example describes it the same way.

The path is:

1. search or entry-point discovery to find candidates,
2. `tiny` cards to orient and route,
3. `normal` cards to understand a neighborhood,
4. `deep` cards only before writing code or when exact source or body details matter,
5. overlay commentary is optional, labeled machine-authored, and freshness-sensitive — request `require_freshness=true` explicitly when it matters.

Do-not rules, asserted uniformly across surfaces:

- do not open large files first;
- do not treat commentary as canonical;
- do not trigger synthesis unless the task justifies it;
- do not expect watch or background behavior unless explicitly enabled;
- **explicit refresh required for fresh commentary**: tools return stale content with tag, never blocking for new synthesis.

The existing context-budget protocol is the substrate for this doctrine; the doctrine makes the protocol visible at every entry point an agent can hit.

### Soft-state lifecycle

Overlay and operational-history surfaces have an explicit lifecycle: create, mark stale, refresh on demand, compact, prune or expire. Semantic compression applies only to these soft surfaces — commentary, cross-link candidates, findings, and recent operational history. Canonical graph data is never compacted semantically and is never replaced by summaries.

Retention rules live in the existing `Retention and compaction` table. The **shipped `synrepo compact` command** enforces these rules by merging logs, rebuilding indexes, and pruning retired nodes older than the retention window. The rule that stays constant: the graph is permanent for its current schema and cannot be collapsed into prose.

### Workflow handoff as a derived surface

synrepo may emit action-oriented handoff items such as "refresh stale commentary," "review high-confidence cross-link," "inspect hotspot pair," or "repair drift surface." These items are:

- derived from repair reports, recent-activity events, overlay candidates and audit rows, commentary freshness, git-intelligence hotspots and co-change partners, and reconcile or export status;
- bounded and ordered — the surface returns a capped, prioritized list, not a full history;
- regeneratable — losing the list costs nothing because the inputs are persisted in their canonical stores;
- exportable to external task systems (JSON or Markdown) without synrepo taking ownership of assignment, status, or collaboration.

Handoff items carry the same provenance fields as cards: `source_store`, `epistemic_status`, and freshness. This prevents them from drifting into a pseudo-graph of recommendations that nobody can verify.

---

## Cards and the context budget protocol

The user-facing centerpiece. The thing an agent actually consumes is not a graph and not a query result — it is a card.

### What a card is

A card is a tiny, structured, deterministic record about one thing in the project, compiled from the graph and source code, designed to fit a specific token budget and answer a specific class of agent question. Cards are not prose summaries. They are *records* — like a library catalog card, like a stat block, like the side panel of a Wikipedia article. A SymbolCard looks like this in spirit (the actual format is JSON returned over MCP):

```
SymbolCard for `parse_query` (function)
  defined at: src/parser/query.rs:142
  signature: pub fn parse_query(input: &str) -> Result<Query, ParseError>
  doc-comment: "Parse a query string into a typed Query AST..."
  source body: available at `deep` budget
  callers/callees: shipped (file-scoped Calls edges; symbol-to-symbol
    precision is still approximate name resolution, not full type-aware
    binding)
  tests touching this symbol: pending dedicated TestSurfaceCard wiring
  last meaningful change: shipped; uses symbol-level granularity by diffing
    `body_hash` transitions across the sampled git history. Tracked by
    `symbol-last-change-v1`.
  approx tokens: 120
```

Every field comes from the graph, syntext, or git. Zero LLM involvement in the card itself. An agent that needs to understand `parse_query` reads this card (~120 tokens) instead of opening `src/parser/query.rs` and consuming the whole file (possibly 4000 tokens).

synrepo compiles several card types. The full family below describes the product direction. As of 2026-04-16, the shipped set is `SymbolCard`, `FileCard`, `ModuleCard`, `EntryPointCard`, `DecisionCard`, `ChangeRiskCard`, `CallPathCard`, and `TestSurfaceCard` (an improvement over the earlier v4 snapshot). `PublicAPICard` remains planned. The connectivity and change-impact fields on shipped cards are now wired; `FileCard.git_intelligence` is fully functional at `Normal` and `Deep` budget.

| Card type | Status | Answers | Compiled from |
| --- | --- | --- | --- |
| **SymbolCard** | shipped | What is this function/class, and how is it connected? | tree-sitter symbol + graph edges; symbol-level call graph is approximate (name-based), not type-aware |
| **FileCard** | shipped | What is in this file, what depends on it? | symbol list + import graph + git history/hotspots |
| **ModuleCard** | shipped | What does this directory do, what is its public surface? | aggregated file / symbol facts for one directory (no recursion into subdirectories) |
| **EntryPointCard** | shipped | Where does execution start in this subsystem? | binary, cli_command, http_handler, and lib_root classification rules over the symbol graph |
| **CallPathCard** | shipped | How does control flow get from A to B? | shortest path in the call graph (limited to file scope in v1) |
| **ChangeRiskCard** | shipped | What breaks if I modify this? | dependents + co-change + drift + hotspot signals (beta fidelity) |
| **PublicAPICard** | shipped | What does this crate/module expose? | export list + visibility analysis + recent API changes |
| **TestSurfaceCard** | shipped | What tests constrain this symbol? | test files discovered via path-convention heuristics |
| **DecisionCard** *(optional)* | shipped | Why was this built this way? | linked human-authored ADRs and inline `# DECISION:` markers |

The first eight card types require zero LLM involvement and zero prose ingestion beyond docstrings and inline comments. They work on a brand-new repo with no documentation. They are the wedge.

### The context budget protocol

The three tiers are not just truncation knobs. They are a deliberate three-surface progressive-disclosure protocol (`progressive-disclosure-v1`): `tiny` is the *index surface* for orientation and routing, `normal` is the *neighborhood surface* for local understanding, and `deep` is the *fetch-on-demand surface* that includes source bodies and optional commentary. Agents are expected to escalate intentionally (index, then neighborhood, then deep fetch), the same shape that good library search APIs take (search → context → fetch). This inverts the RAG default of "return everything similar and hope the right thing is in there."

Every MCP tool that returns cards declares a token budget, and the agent picks the tier:

| Tier | Budget | What the agent gets |
| --- | --- | --- |
| `tiny` | ~200 tokens per card, ~1k total | Card headers plus whichever connectivity fields are currently compiled |
| `normal` | ~500 per card, ~3k total | Full current card payload for local understanding |
| `deep` | ~2k per card, ~10k total | Full card plus actual source body, plus linked DecisionCards if available |

Default is `tiny`. The SKILL.md tells agents: use `tiny` to orient and route, use `normal` when you need to understand a specific symbol, use `deep` only when about to write code that depends on the exact source. This inverts how RAG usually works — instead of returning everything similar and hoping the right thing is in there, synrepo returns the smallest accurate answer and lets the agent ask for more.

Token budgets are enforced server-side by trimming lower-priority optional fields as richer card surfaces land. Agents never see surprise token blowouts.

### Why cards are not summaries

A summary is LLM prose describing a thing. A card is a structured record about a thing. Summaries are unverifiable, drift fast, and require LLM compute. Cards are checkable against the graph at any time, regenerate in milliseconds when source changes, and require zero LLM involvement for their primary fields. Cards are what RAG and "wiki summaries" were trying to be, with the LLM removed from the critical path.

If an agent needs a richer card than the structural data supports — say, "what is the design intent of this module" — synrepo optionally generates an LLM-written commentary tier on top of the structural card. The commentary lives in the overlay store, is clearly labeled machine-authored, and is *additive*, never replacing structural fields. The structural card is the trustworthy core; commentary is convenience.

---

## Architecture

Four layers, bottom to top.

**Substrate layer.** syntext as a Rust library dependency. Provides the n-gram index over the entire corpus — code files, markdown files, everything textual. Provides `commit_batch` semantics so structural updates and citation verification are consistent within milliseconds.

**Structure layer.** The unified graph for code and prose. Code symbols from tree-sitter via per-language Rust crates. Prose links from a Markdown parser (standard links, wiki-links, anchors, frontmatter). The graph store is sqlite, single source of truth, no in-memory mirror in v1. SQLite handles two-hop queries in milliseconds and is fine for the vast majority of MCP traversals. If a specific query class crosses a measured latency threshold, an in-memory petgraph layer can be added *later* for that specific query type without rearchitecting. Single source of truth saves debugging weeks.

**Overlay layer.** LLM-authored content lives here, physically separate from the graph. The overlay holds proposed cross-links (with cited evidence and confidence), optional card commentary tiers, and findings. The overlay is queryable via MCP so agents can see it, but the synthesis pipeline never reads it as input for subsequent passes. This is the contamination invariant.

**Surface layer.** The CLI, the MCP server, and a thin skill bundle for Claude. The CLI is for humans and CI. The MCP server is a separate stdio server that serves cards and graph primitives to agents. Optional watch mode is a per-repo local service started explicitly with `synrepo watch` or `synrepo watch --daemon`. The skill is a discoverability layer that tells Claude when to reach for the MCP server.

---

## Data model

The graph holds only what was directly observed. Everything inferred lives in the overlay.

### Graph nodes (canonical)

Three node types, all with `epistemic_status` ∈ `{parser_observed, human_declared, git_observed}`. Machine-authored content does not exist in the graph.

- **File nodes** — anything on disk. Identity is content-hash plus path-history, with AST-based rename detection as the primary mechanism.
- **Symbol nodes** — functions, classes, methods, types, exports. Extracted via tree-sitter queries. Identity is `(file_node_id, qualified_name, kind, body_hash)`.
- **Concept nodes** — *only* created from human-authored Markdown files in configured directories (default `docs/concepts/`, `docs/adr/`, `docs/decisions/`). **The synthesis layer cannot mint concept nodes in any mode.** For vibe coder repos with no concept directories, concept nodes simply don't exist — and that's fine, because cards cover the common case without needing an ontology layer.

### Graph edges (canonical)

Restricted to observed types: `imports`, `calls`, `inherits`, `defines`, `references` (parser-observed); `mentions` (markdown link parser); `co_changes_with` (git-observed); `governs` (only when declared in ADR frontmatter or inline marker, never inferred). Each edge carries provenance, an `epistemic_status` from the observed-only spectrum, and a `drift_score` updated on every commit by the structural pipeline.

### Overlay contents

LLM-authored content lives in `.synrepo/overlay/` with `epistemic_status` ∈ `{machine_authored_high_conf, machine_authored_low_conf}`:

- **Proposed cross-links** — LLM-suggested edges with cited evidence verified by syntext, confidence scores, fuzzy-matched source spans.
- **Card commentary** — optional LLM prose layered on top of structural cards when requested.
- **Findings** — inconsistencies, stale rationale candidates, contradictions.

The overlay is queryable via MCP — `synrepo_card(target, deep)` returns structural card + overlay commentary (clearly labeled). The synthesis pipeline filters overlay content out of its retrieval inputs.

### Recent-activity surface

A bounded lane for "what has synrepo done recently?" — not session memory, not agent-interaction history, not a replacement for `git log`. It surfaces *synrepo's own operational events* so an agent can orient without re-reading everything after a stale period. **Shipped in v1 via `synrepo_recent_activity` tools and `status --recent`.**

What it returns:

- recent reconcile outcomes (timestamps, file-count delta, duration, success/failure)
- recent `repair-log.jsonl` entries (drift surface, severity, action taken)
- recent cross-link accept/reject decisions from the overlay
- recent commentary refreshes with their content-hash freshness state
- recent churn-hot files derived from the already-mined Git intelligence

What it explicitly is not:

- a persistent record of what agents read, asked, or wrote
- cross-session memory of prior conversations
- a vehicle for reintroducing auto-captured activity that would blur the graph-versus-overlay boundary

The data already exists: `.synrepo/state/reconcile-state.json`, `.synrepo/state/repair-log.jsonl`, the overlay cross-link store, the overlay commentary store, and `pipeline::git_intelligence` hotspot output. This is a surface-layer exposure of persisted operational events, not new telemetry. See `synrepo_recent_activity` in FOUNDATION-SPEC §12.

---

## The two pipelines

**Structural pipeline (hot path, no LLM).** Runs on every change, synchronously, seconds even on thousand-file refactors. Walks the configured roots, parses code via tree-sitter (supporting Rust, Python, TypeScript/TSX, and Go), parses prose via the Markdown parser, mines git history, computes derived structural facts (reachability, dead code, hidden coupling, drift scores), commits to sqlite and syntext. Stage 4 resolution is **scoped and scored (`stage4-call-scope-narrowing-v1`)**, using a rubric (same file, imports, visibility, kind, prefix) to drastically narrow `Calls` edges. Visibility (`Public`, `Crate`, `Private`) is a first-class, cross-language field (`cross-language-symbol-visibility-v1`). Changed files are upserted in place (preserving stable node identity), and stale observations are soft-retired rather than cascade-deleted, so drift scoring and provenance remain coherent across revisions. That's the whole critical path — no cascade budget, no deferral, no nightly queue. A 1,000-file refactor gets its graph updated in a few seconds of tree-sitter plus the graph write. Agents never read stale structural state.

**Synthesis pipeline (cold path, LLM-driven, lazy).** Never blocks the structural pipeline. Never blocks MCP queries. Runs in three triggering modes: on-demand (an MCP tool asked for commentary or a card at `deep` tier), background (low-priority worker regenerating overlay during idle time), or explicit (the user ran `synrepo sync --generate-cross-links`). Produces card commentary, proposes cross-links, runs lint. Everything it produces goes into the overlay. Input never includes other overlay content, enforced at the retrieval layer.

*Current shape.* `src/pipeline/synthesis/` defines two trait boundaries: `CommentaryGenerator` and `CrossLinkGenerator`. Default installs use `NoOpGenerator` and `NoOpCrossLinkGenerator` (both return empty results), so the product stays deterministic and LLM-free out of the box. Setting `SYNREPO_ANTHROPIC_API_KEY` swaps in `ClaudeCommentaryGenerator` and the Claude-backed cross-link generator. The trait boundary is a real improvement over the earlier "stub" posture: it lets an operator opt into synthesis without threading a new code path through the rest of the pipeline, and it keeps every test fixture LLM-free by default.

Staleness is explicit. Every overlay entry carries the content hash of the sources it was generated from. If the sources have changed, the card response marks the entry as `stale`. Agents can request fresh synthesis explicitly via the `synrepo_refresh_commentary` tool. **The default behavior is non-blocking stale retrieval; lazy background synthesis is not part of the v1 contract to keep the read path deterministic.**

---

## Cross-linking with evidence verification

How LLM-proposed cross-links get from "a candidate pair looks promising" to "an overlay entry with verified citations." Four stages.

*v1 implementation.* Stages 1 through 4 below are fully shipped. Stage 1 utilizes a deterministic name-match prefilter by default, with an opt-in **semantic triage (`semantic-triage-v1`)** feature backed by local ONNX models to catch relationship candidates that lack lexical overlap.

### Stage 1 — Hybrid candidate generation

Pure embedding similarity is bad at topological scoping. An embedding search for "auth pipeline" returns files from 6 unrelated services. Candidate generation is therefore hybrid in three dimensions: semantic (embeddings), topological (graph hops), and locality (directory distance). Default scoring:

```
score = 0.5 * embedding_cosine
      + 0.3 * 1/(1 + graph_hops)
      + 0.2 * 1/(1 + directory_distance)
```

The default embedding model is `all-MiniLM-L6-v2` via the `ort` ONNX runtime crate — ~90 MB on disk, CPU-fast, 384-dimensional, fully local. Vectors are keyed by `(content_hash, model_id)` and cached in `.synrepo/embeddings/`. This semantic prefilter acts as a safety net for candidate pairs that fail the deterministic prefilter.

For each source artifact, retrieve top 50 by raw embedding similarity, re-rank by the hybrid score, take top K (default 20). None of these land in the graph; they are candidates for the LLM to consider.

### Stage 2 — Two-stage LLM triage

Passing 20 full candidate files to the LLM is 50–100k tokens per call and destroys the cost budget. Split the LLM work:

**Stage 2a — Signature triage.** The LLM sees the source artifact in full plus *structural signatures* of the 20 candidates (path, top-level symbol names with one-line signatures, doc-comment first lines, frontmatter). A few hundred tokens per candidate. The LLM picks 2–3 it wants to inspect in detail. Returning an empty selection is fine and cheap.

**Stage 2b — Full-source proposal with citations.** For the 2–3 selected candidates, the LLM sees the full source and is asked to propose a typed link *only if* it can cite specific verbatim spans in both artifacts. Strict output schema:

```json
{
  "link_type": "references" | "governs" | "derived_from" | "mentions" | null,
  "source_spans": [{"span": "verbatim text", "byte_range": [n, m]}],
  "target_spans": [{"span": "verbatim text", "byte_range": [n, m]}],
  "rationale": "one-sentence explanation"
}
```

If the LLM cannot cite, it returns `null` and no link is proposed.

Two-stage triage cuts per-artifact cost by roughly 80% versus the naive approach.

### Stage 3 — Normalized fuzzy verification

Verification is lexical, deterministic, but **normalized and fuzzy-matched**, not byte-exact. Two layers of robustness:

**Normalization layer.** Both the LLM's cited span and the candidate window from the source artifact are passed through the same normalizer: collapse whitespace runs, normalize line endings, normalize Unicode quote variants to ASCII, normalize Unicode dashes, lowercase the first character of the span.

**Fuzzy token match.** After normalization, compare via a highly efficient **3-stage cascade (`repair-fuzzy-match-algorithm-v1`)** bounded by a soft time budget:
1. **Exact substring match** — for high-fidelity frontier model output.
2. **Anchored partial match** — for cases where the LLM truncated or slightly altered the start/end.
3. **Windowed LCS** — a sliding-window longest common subsequence at a configurable threshold (default 90%). 

If any stage succeeds, the citation is accepted and the edge is **snapped to the actual byte range of that window**, not the LLM's claimed offset. The threshold is provider-tunable — frontier models (Claude, GPT-4 class) use 95%, local models (Llama-3, Phi) use 90% or lower.

Why fuzzy is OK: the point isn't to prove the LLM produced a perfect transcription, it's to prove the substantive content the LLM claims exists in the source actually exists. A 90% token match means the LLM correctly identified a real passage and slightly mangled the surface form. Snapping to the actual span means the overlay records the truth, not the approximation.

Then verification proceeds: does the target node exist? Do both cited spans pass fuzzy match against the real artifacts? Is the link type allowed for this pair of node types? Pass all four and the link is written to the overlay with a confidence score derived from span count, span length, LCS ratios, and graph distance.

### Stage 4 — Lint and classification

A final LLM pass scans the new overlay entries for contradictions, cluster anomalies, and low-evidence patterns (same generic rationale across many candidates is a signature of lazy proposal). Findings go to `findings.md` in curated mode or the overlay `findings/` store in auto mode. Never auto-applied.

### What evidence verification actually proves

Cited-evidence verification is a strong improvement over "the target node exists," but it proves the citations are real, not that the inferred semantic relationship is correct. An LLM can cite two real spans from two real documents and still draw the wrong relationship. The defenses are layered and partial: link-type allowlists, confidence scoring penalizing graph distance, lint-pass anomaly detection. The honest framing is that evidence-based verification turns silent failures into rare ones, and rare ones into ones that show up in the findings feed. It is much better than the alternative and it is not perfect.

---

## Identity and stability

AST-based rename detection as the primary mechanism, with content hash and `git log --follow` as fallbacks. This is the single most important correctness problem: if file node identity breaks, every inbound edge breaks and the graph rots.

**File identity.** `FileNodeId` is stable across renames, splits, merges, and in-place content edits (`structural-resilience-v1` & `v2`). For new files, it is derived from the content hash of the first-seen version; for existing files, the stored ID is always reused. A content-hash change advances the `content_hash` version field on the file node without triggering node deletion or invalidating inbound edges. This prevents the graph from rotting during rapid local saves.

When the structural pipeline detects a file disappearance and one or more new files in the same compile cycle, it runs the **Identity Cascade (shipped in Stage 6)**:

1. **AST symbol-set match.** For each new file, compute the set of `(qualified_name, body_hash)` tuples for its top-level symbols. If a disappeared file's symbol set overlaps substantially (threshold configurable), treat it as a rename: preserve the file node ID, append the new path to path history, log the rename. Catches simple renames cleanly.
2. **Symbol-set split.** If a disappeared file's symbols are split across multiple new files (e.g. `auth.rs` -> `jwt.rs` + `session.rs`), split the file node. The original retains its ID and points to the largest-overlap new file; a new node is created for the other with `split_from` provenance edges. Refactors heal automatically when there's structural evidence.
3. **Symbol-set merge.** Symmetric: multiple disappeared files' symbols all in one new file -> new node with `merged_from` edges.
4. **Git rename fallback.** When symbol evidence is inconclusive, fall back to a deterministic `gix` rename detection pass.
5. **Accept breakage as last resort.** When neither symbol nor git evidence connects an old file to a new one, treat it as delete + add. Log a finding. The system is designed to tolerate this gracefully: broken edges become candidates for the next synthesis pass.

Physical deletion is reserved for files genuinely absent from the repository after the identity cascade, and for the compaction maintenance pass that removes retired observations older than the configured retention window (`retain_retired_revisions`, default 10 compile revisions).

**Symbol identity.** `(file_node_id, qualified_name, kind, body_hash)`. A body rewrite updates the hash but keeps the node ID. A name change is detected via AST-match within-file.

**Observation lifecycle.** Every parser-emitted symbol and edge carries an `owner_file_id` (the file whose parse pass produced it) and observation-window fields (`last_observed_rev`, `retired_at_rev`). This **soft-retirement lifecycle (`graph-lifecycle-v1`)** ensures that stale observations are marked retired rather than cascade-deleted, preserving the audit trail. Recompiling a file retires only observations owned by that file that were not re-emitted, leaving observations owned by other files untouched. Human-declared facts (`Epistemic::HumanDeclared`) are never retired by a parser pass. Retired observations remain physically present and visible to drift scoring (shipped in Stage 7) until compacted. Drift scoring uses Jaccard distance on persisted structural fingerprints in the sidecar `edge_drift` table.

This is much stronger than a git-only approach but it is not bedrock. Pathological refactors (path moves + symbol renames + body rewrites in one commit) will still defeat it. The system degrades gracefully: broken edges become findings, the synthesis pipeline can re-propose them lazily.

---

## File type handling

| Class | Handling |
| --- | --- |
| Code in a supported language (Rust, Python, TypeScript/TSX, Go) | Full pipeline: index + tree-sitter parse + symbol extraction |
| Other text code (toml, yaml, sql, shell) | Index only, no symbol extraction |
| Markdown / mdx | Index + link parse + frontmatter parse |
| Jupyter notebooks | Extract source cells, ignore output cells |
| PDF | Extract text via `pdf-extract`, index, no structure (phase 6) |
| Images | Skip in v1 |
| Binary | Skip |
| Symlinks | Follow once, detect cycles, never outside configured roots |
| Hidden files | Skip except `.github/`, `.claude/`, configurable additions |

**Encoding.** Sniff UTF-8, fall back to UTF-8-with-BOM, then refuse the file. Never silently transcode.

**Very long lines.** Cap parsing (not indexing) at 100k bytes per line for minified JS, generated SQL, and similar.

---

## Git integration gotchas

- **Worktrees.** Use `git rev-parse --git-dir`, never assume `.git/` is in cwd.
- **Bare repos.** Refuse to run. No working tree to compile.
- **Submodules.** Ignored by default; opt-in as separate corpora.
- **Shallow clones.** History mining degraded; detect and warn.
- **LFS.** Skip pointer files.
- **Large binary files.** Skip anything that fails UTF-8 sniff or exceeds 1 MB default.
- **`.gitignore` semantics.** Respect `.gitignore`, `.git/info/exclude`, global excludes, plus `.synignore` for synrepo-specific exclusions.
- **Generated and vendored files.** Ignore `node_modules/`, `target/`, `vendor/`, `dist/`, `__pycache__/` by default.
- **History rewrites.** Detect force-push via most recent ref hash; invalidate git cache on mismatch.
- **Detached HEAD.** Works fine; record commit hash, not branch name.
- **Mixed line endings.** Normalize to LF for hashing so `core.autocrlf=true` Windows checkouts don't churn the index.

---

## The MCP tool surface

Task-first and card-centric. The default response unit is a card (or set of cards) sized to a token budget.

### Task-shaped tools (primary)

| Tool | Status | Purpose | Returns |
| --- | --- | --- | --- |
| `synrepo_overview(budget?)` | shipped | First-call orientation on an unfamiliar project | graph counts today; ModuleCards / EntryPointCards to fold in |
| `synrepo_card(target, type?, budget?, require_freshness?)` | shipped for symbol / file / concept; directory case planned | Card for a specific symbol, file, or module | The requested card at the specified tier |
| `synrepo_module_card(path, budget?)` | shipped | Directory-targeted ModuleCard; standalone-usable, also exposed via MCP | ModuleCard at the specified tier |
| `synrepo_where_to_edit(task_description, budget?)` | shipped | "I want to do X, which files matter?" | Ranked FileCards from lexical matches plus lightweight structural signals |
| `synrepo_change_impact(target, budget?)` | shipped | "If I modify this, what could break?" | Approximate impacted files today; fuller ChangeRiskCard shape later |
| `synrepo_entrypoints(scope?, budget?)` | shipped | "Where does execution start?" | EntryPointCards for the scope |
| `synrepo_call_path(from, to, budget?)` | shipped | "How does control flow get from A to B?" | CallPathCard with shortest path |
| `synrepo_test_surface(target, budget?)` | shipped | "What tests constrain this behavior?" | TestSurfaceCard |
| `synrepo_minimum_context(task_description, budget?)` | shipped | "Smallest file set for this task?" | Budget-capped ranked FileCards |
| `synrepo_next_actions(limit?, since_days?)` | shipped | prioritized, derived handoffs from repair log, overlay, and git hotspots | prioritized task list |
| `synrepo_public_api(path, budget?)` | shipped | "What is the public surface of this module?" | PublicAPICard |
| `synrepo_search(query)` | shipped | Exact n-gram search via syntext | Lexical fallback when name-based lookup fails |
| `synrepo_findings(scope?)` | shipped | Overlay findings and inconsistencies | Findings with provenance |

Every tool that returns cards takes a `budget` parameter (`tiny` | `normal` | `deep`, default `tiny`). Budgets are enforced server-side.

### Low-level primitives (debugging and power use)

| Tool | Purpose |
| --- | --- |
| `synrepo_node(id)` | Raw graph lookup |
| `synrepo_edges(id, direction?, types?)` | Raw edge traversal |
| `synrepo_query(graph_query)` | Structured query over the graph |
| `synrepo_overlay(target)` | Raw overlay lookup for a target |
| `synrepo_provenance(id)` | Full provenance chain |

Use only when task-shaped tools aren't returning what's needed.

### Freshness and blocking behavior

Every card response is tagged with `epistemic_status` (per field) and `source_store` (`graph` or `overlay`). Graph-sourced fields are always fresh. Overlay-sourced commentary is tagged `fresh | stale | missing`.

**Default is non-blocking.** Tools return current cached content immediately with a staleness tag and fire background synthesis for stale items. The agent passes `require_freshness=true` explicitly when about to write code that depends on fresh commentary. The SKILL.md tells the agent when to escalate. Autonomy goes to the agent because the agent knows what task it is performing; the tool does not.

### Inspectability

Trust requires visible state. synrepo treats inspection surfaces as first-class, not debug output:

- Every card response is labeled per field with `source_store` (`graph` | `overlay`) and `epistemic_status`, plus freshness state for overlay content. A caller that ignores these labels is visibly choosing to.
- `synrepo status [--json]` is the operator-facing health surface: mode, counts, last reconcile, writer lock, watch ownership, export freshness, overlay cost. It is meant to be read, not parsed through tracing output.
- `synrepo_recent_activity` (planned, see above) is the agent-facing history surface for the same data an operator would read from `synrepo status --recent`.
- Watch ownership and export freshness are exposed here because invisible background behavior is the specific failure mode synrepo refuses to accept.

---

## Trust hierarchy and conflict resolution

Two hierarchies for two questions.

### Descriptive ("what does the code do")

1. Code (`parser_observed`)
2. Tests (`parser_observed`)
3. Inline source markers (`human_declared`)
4. Co-change patterns (`git_observed`)
5. README and design docs (`human_declared`) — secondary for this question
6. ADR files (`human_declared`) — secondary
7. Overlay content — supplements only

### Normative ("why does the code exist")

1. Inline source markers (`human_declared`) — strongest, both human-authored and proximate
2. ADR files (`human_declared`) — strong intent, even when stale relative to current code
3. README and design docs (`human_declared`)
4. Commit messages (`git_observed`)
5. Code (`parser_observed`) — weak for this question; tells you what is, not what was intended
6. Tests (`parser_observed`) — slightly stronger than non-test code
7. Overlay content — supplements only

### Conflict rules

- **Code contradicts an ADR.** Descriptive: code wins, ADR marked stale via drift score. Normative: ADR still wins as intent, with drift annotation. Agent sees both.
- **Inline marker contradicts an ADR.** Inline wins for both questions. Proximity is precision.
- **Two human-declared sources conflict directly.** Both surfaced, finding logged, no silent winner.
- **Overlay never overrides graph.** Ever. Overlay is supplemental. Conflicts silently drop the overlay entry and log.
- **Within the graph, higher-trust labels override lower.** Parser observations upgrade git-observed inferences in place.

Every resolution writes to `resolutions.log`.

---

## Reuse strategy: syntext, tree-sitter, queries

**syntext** as a Rust library dependency, pinned by version. Verify syntext's license before writing code: if AGPL, synrepo inherits; if permissive, synrepo picks its own.

**tree-sitter** via the Rust crates, not the JS pipeline. The ecosystem has a well-documented packaging mess on the Node side ([Ayats, 2024](https://ayats.org/blog/tree-sitter-packaging)). The Rust side sidesteps it entirely: each language grammar is a standalone crate (`tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-typescript`) that ships a pre-generated C blob and exposes a `language()` function. No Node, no npm, no vendoring.

**Query files from the crates directly.** Modern `tree-sitter-<lang>` crates expose `HIGHLIGHTS_QUERY`, `INJECTIONS_QUERY`, and `LOCALS_QUERY` as `&'static str` constants — the queries are bundled with the grammar and MIT-licensed alongside it. synrepo reads them via the crate API and adds small per-language `extra.scm` files on top for synrepo-specific captures (docstring spans, decorator metadata, doc-comment positions). No vendoring, no PROVENANCE.md, no AGPL contamination concerns. The thin per-language adapter is the only synrepo-owned query content.

Caveats: not every grammar crate exposes the constants uniformly (some older or community-maintained crates lag); pin specific versions and CI the merged queries against representative source files per language; TypeScript ships two grammars (`language_typescript()` and `language_tsx()` — pick per file extension); first build compiles non-trivial C blobs per language.

---

## Storage layout

```
.synrepo/                       # gitignored except config.toml and .gitignore
  config.toml                   # checked into git
  .gitignore                    # the per-dir gitignore the init writes
  index/                        # syntext segments (shipped)
  graph/                        # canonical: parser/git/human facts only
    nodes.db                    # nodes, edges, provenance in one SQLite file
  overlay/                      # machine-authored content, physically separate
    overlay.db                  # commentary, proposed links, findings in one SQLite file
  embeddings/                   # reserved for Phase 5 embedding cache (empty today)
  cache/                        # LLM response cache directory (empty today)
  state/
    reconcile-state.json        # last reconcile outcome, timestamp, counts
    repair-log.jsonl            # append-only resolution log written by `synrepo sync`
    storage-compat.json         # storage compatibility snapshot
    writer.lock                 # (conditional) process-level write lock, PID + timestamp
    watch-daemon.json           # (conditional) watch owner lease + telemetry
    watch.sock                  # (conditional) local control socket for active daemon
```

*Improvement vs the split-store v4 sketch.* Nodes, edges, and provenance live in a single `nodes.db`; commentary, cross-links, and findings live in a single `overlay.db`. Rationale: multi-table reads inside one SQLite file open under a single `BEGIN DEFERRED` snapshot, which is how the reader-snapshot invariant (invariant 8 in `CLAUDE.md`) can hold. Splitting would force either cross-DB attach contortions or coordinated per-store snapshots. The overhead of one extra file isn't worth fracturing atomicity. The graph and overlay stores remain physically separated from each other; the contamination invariant is untouched.

*Laziness drift to call out.* The `cache/` and `embeddings/` directories are created but unused in the default install. They are not placeholders for imagined features; they are the hook points for Phase 5 embeddings and the Phase 4 LLM response cache respectively. Either wire them to the existing opt-in Claude generators or drop them when Phase 5 lands; do not let them accumulate as unexplained empty directories.

The graph store is canonical. The overlay is physically separate. Nothing in `.synrepo/` is committed except `config.toml` and `.gitignore`.

---

## Concurrency model

Single writer, many readers. The stdio MCP server is an agent-facing read surface, not the daemon or authoritative writer. Standalone CLI commands remain the default operational path and acquire `.synrepo/state/writer.lock` only for the duration of an actual write. Optional watch mode is a separate per-repo service started explicitly by the user.

When watch mode is active, `.synrepo/state/watch-daemon.json` records the watch owner and recent telemetry, and `.synrepo/state/watch.sock` provides a local control socket for `status`, `stop`, and `reconcile_now`. The watch lease is long-lived. `writer.lock` remains operation-scoped and still guards each actual mutate step, including watch-triggered reconcile passes.

The watch service runs a startup reconcile before attaching steady-state watching. File watching uses `notify` for cross-platform inotify/kqueue/ReadDirectoryChangesW. Events are debounced (default 500 ms), batched, and filtered so `.synrepo/` runtime writes do not trigger self-induced reconcile loops. Watch mode stays a trigger and coalescing layer over the deterministic reconcile path rather than a second source of graph truth.

---

## Auditability and provenance

Every graph row and every overlay entry carries provenance:

```yaml
provenance:
  created_at: 2026-04-09T14:23:01Z
  source_revision: a1b2c3d4
  created_by: structural_pipeline | synthesis_pipeline | human | bootstrap
  pass: parse_code | parse_prose | git_mine | propose_link | summarize | ...
  source_artifacts:
    - id: file_0042
      path: src/auth/middleware.rs
      content_hash: sha256:...
  # synthesis rows only:
  prompt_version: v3.1
  model: claude-sonnet-4-6
  cost_tokens: 1843
  cited_spans:
    - artifact_id: file_0042
      normalized_text: "..."
      verified_at_offset: 1234
      lcs_ratio: 0.94
  confidence: 0.87
```

Verbose on purpose. Auditability is the value proposition versus RAG. Every cross-link can be traced to specific cited spans in specific source artifacts at a specific revision, with the model and prompt version that produced it.

---

## The gotchas, ranked

1. **Wrong links to real nodes with real citations.** Cited-evidence verification proves citations are real, not that the inferred relationship is correct. Defenses: link-type allowlists, confidence scoring, lint anomaly detection, overlay-not-graph placement. Not bulletproof.
2. **The verbatim citation trap.** LLMs alter whitespace, expand tabs, normalize endings, capitalize quote starts, drop punctuation, smarten quotes. Byte-exact matching produces catastrophic false-rejection. Defense: normalization + fuzzy LCS (default 90%, provider-tunable) + snap-to-actual-span.
3. **Context window explosion in cross-linking.** Naively passing 20 full candidates is 50–100k tokens. Defense: two-stage triage (signatures → full source for 2–3 picks). ~80% cost reduction.
4. **Stale-summary agent loops.** Agents reflexively calling synthesize on every stale item. Defense: non-blocking by default, `require_freshness=true` opt-in, SKILL.md guidance.
5. **Empty-repository bootstrap.** Code-only repos have no prose side. In auto mode, this is fine because cards cover the common case; concept nodes simply don't exist. No canonical/overlay invariant breakage.
6. **Feedback contamination.** If synthesis reads its own output, errors compound. Defense: physical separation (graph vs overlay tables) plus retrieval-layer filter on synthesis input.
7. **Canonical vs overlay leakage.** Agents may blur the line. Defense: MCP responses clearly label every field with `source_store` and `epistemic_status`; behavioral metrics track whether agents actually respect the labels.
8. **Ontology sprawl from auto-minted concepts.** Defense: concept nodes are only ever human-declared, in any mode. Cards cover the common case for vibe coders.
9. **Cost runaway.** `max_cost_per_synthesis_cycle` hard stop; LLM response cache; two-stage triage; zero-temperature lint.
10. **Synthesis write loops.** Content-hash check before every overlay write; `.synrepo/` excluded from the watcher's observed set.
11. **Stale ADRs.** Drift scoring on `governs` edges, computed by the structural pipeline on every commit, surfaced in card responses. Agent sees drift without any LLM work.
12. **Heavy-refactor identity breakage.** AST-based detection handles splits, merges, simple renames. Pathological cases produce findings and lazy re-linking.
13. **Encoding edge cases.** Sniff and skip; never panic.
14. **PII and secrets.** `.synrepo/redact.toml` for path globs; defaults skip `**/secrets/**`, `**/*.env*`, `**/*-private.md`; embedding respects same redaction.
15. **Embedding provider drift.** Cache keyed by `(content_hash, model_id)`; model swap triggers targeted re-embed.
16. **Index format migrations.** Version every store independently; ship migration code.
17. **Multi-root projects.** `synrepo init` asks explicitly — one synrepo per subproject or one combined.
18. **Cross-repo references.** Out of scope for v1. Documented.
19. **The watcher missing events under load.** Full reconcile pass on startup and reconcile backstop after coalesced bursts.
20. **Tree-sitter version skew.** Pin grammar versions; CI against representative sources.
21. **Symlink loops.** Detect during discover; never recurse twice.
22. **Concurrent CLI invocations.** Watch ownership is separate from `writer.lock`; unsupported mutating commands fail fast while watch is active, and standalone writes still use the file lock when watch is inactive.
23. **When the LLM is wrong.** It will be. Lint pass and confirmation on high-impact changes are the safety net.

---

## Unresolved risks

Real problems the design does not fully solve, listed honestly.

**Identity stability under heavy refactor.** AST-based detection raises the floor; pathological refactors still defeat it. Degrades gracefully but produces noisy findings.

**Wrong-but-cited links.** Evidence verification proves citations exist, not that inferred relationships are correct. Small residual failure rate even when all four stages pass.

**Vibe coder trust drift.** Auto mode populates the overlay with machine-authored content that agents can see. If agents (or users) ignore the epistemic labels and treat overlay content as ground truth, trust erodes. Defense is physical separation and clear labeling; enforcement is behavioral and measurable but not structural.

**Embedding model lock-in.** Model changes trigger re-embed. Correct (cache keyed by model ID) but not free.

**Long-tail language support.** v1 ships Rust, Python, TypeScript. Adding a language is a real cost: write the `extra.scm`, test against representative source, handle quirks. Languages without good upstream `locals.scm` in the crate require more work.

**Cross-repo linking.** Out of scope. A multi-repo workspace cannot have synrepo-managed links between components.

**LLM provider availability.** Hosted LLM outages halt synthesis. Structural pipeline keeps running; agents continue to get fresh graph facts; overlay refresh halts. Fully-local Ollama backend mitigates for users with local infrastructure.

**Security model is single-user-local in v1.** Mixed-sensitivity monorepos where different users should see different parts of the graph are out of scope. The MCP server runs as the local user with local filesystem permissions.

**Semantic drift not caught by structural drift scoring.** Drift scoring catches prose rot when linked code changes structurally. It will miss cases where code stays structurally similar while meaning changes, or where rationale becomes obsolete because surrounding architecture moved on. Phase 6 may add a "decision relevance decay" signal based on dependency topology and change traffic, but v1 only has structural drift.

**Category drift.** The quiet risk. Adjacent tools in the agent-tooling space make different product bets — session memory, hook-based auto-capture, vector-first retrieval — and their UX patterns are often more immediately magical than synrepo's. The failure mode is absorbing those patterns until synrepo stops being a repo context compiler and becomes a muddier hybrid. Explicit refusals, kept in both FOUNDATION-SPEC §4 non-goals and ROADMAP §9: no generic session memory, no hook-heavy auto-capture as a core value, no invisible background behavior (watch stays explicit and per-repo), no vector-first retrieval as a core dependency, nothing that weakens the graph-versus-overlay separation. Borrowing the progressive-disclosure UX pattern is fine; copying the product center is not.

---

## Evaluation and acceptance criteria

### Structural pipeline (deterministic)

| Metric | Target |
| --- | --- |
| Cold-start indexing, 10k-file repo | < 60 seconds |
| Incremental rebuild, single-file edit | < 500ms p99 |
| Incremental rebuild, 100-file refactor | < 5 seconds p99 |
| MCP card response (graph-only) latency | < 50ms p99 |
| Memory footprint, idle, 10k files | < 500 MB RSS |
| Identity stability, simple renames | 100% |
| Identity stability, split refactors with ≥80% symbol overlap | ≥ 95% |

### Synthesis pipeline (probabilistic)

| Metric | Target |
| --- | --- |
| Citation hallucination rate (post fuzzy match) | < 5% |
| Cross-link precision (survives verification AND human-correct) | > 80% |
| Cross-link recall (correct links missed) | > 60% |
| Cost per synthesis cycle, 1000-file repo | < $5 (Sonnet) / < $0.50 (local) |
| Wrong-but-cited link rate | < 2% |

### Agent task metrics (the ones that actually matter)

| Metric | Target |
| --- | --- |
| Time-to-first-useful-result on `synrepo init` | < 90 seconds |
| Agent task success rate with synrepo vs without | +20% absolute |
| Token cost per agent task | −30% vs cold-context baseline |
| User retention at 2 weeks | > 50% |

### Behavioral metrics (do agents respect the labels?)

- **Overlay-content reliance in high-stakes actions.** What fraction of agent code-write decisions are grounded in overlay content vs graph content? High overlay reliance is a red flag.
- **`require_freshness=true` rate before writes.** Never-invoked means the SKILL.md is failing; always-invoked means defaults are wrong.
- **Overlay contradiction rate.** How often machine-authored overlay content gets later contradicted by a graph fact.
- **Card budget escalation rate.** Fraction of requests escalating `tiny` → `normal` → `deep`. Too high means `tiny` isn't useful; too low means `tiny` is too sparse.

### Anti-metrics

- Findings growth rate vs resolution rate
- Background synthesis queue depth trend
- Median drift score trend
- `.synrepo/` disk growth per week

### Benchmark realism

Subsystem benchmarks lie. The validation suite must include ugly repos: huge generated files, vendored dependencies, weird encodings, shallow clones, partially broken syntax, refactors with thousands of symbol movements, extremely long lines. Linux kernel, Rust compiler, TypeScript codebase, plus a real-world polyglot monorepo.

---

## Retention and compaction

| Store | Retention | Trigger |
| --- | --- | --- |
| `index/` | Compacted on full reindex | Monthly default |
| `graph/` | Permanent for current schema | Migration only |
| `embeddings/` | LRU at 2 GB cap | Size threshold |
| `cache/llm-responses/` | LRU at 1 GB cap | Size threshold |
| `overlay/commentary/` | Keep current; history 30 days | Time |
| `overlay/cross_links.db` | Expire unpromoted low-confidence at 90 days | Time |
| `findings.md` | Append; archive monthly | Time |
| `resolutions.log` | Rotate at 100 MB | Size |

`synrepo compact` is a manual command (users or CI run it). **Shipped in v1.** It merges sqlite WAL, rebuilds indexes, drops orphaned rows, recomputes drift scores. Disk budget warning fires at 80% of cap; writes block at 100% until compact or cap raise.

---

## Phased build plan

The key principle: agent usefulness arrives at phase 2 with zero LLM involvement.

**Phase 0 — substrate (shipped).** syntext as library dependency. Discover-and-index pipeline with file type handling and encoding robustness. CLI: `synrepo init`, `synrepo search`. No graph, no LLM.

**Phase 1 — structural graph (fully shipped).** tree-sitter parsing via crate-bundled queries plus per-language `extra.scm`. sqlite graph store with observed-only epistemic labels and provenance. Stage 6 split/merge/rename detection, Stage 7 drift scoring, and Stage 8 `ArcSwap<Graph>` snapshot publication are shipped. Structural compile pipeline is LLM-free and synchronous.

**Phase 2 — cards and the MCP server (shipped).** Shipped card compiler: SymbolCard, FileCard, ModuleCard, EntryPointCard, DecisionCard, PublicAPICard, CallPathCard, TestSurfaceCard, ChangeRiskCard. Context budget protocol with `tiny` / `normal` / `deep` tiers and server-side enforcement shipped. Shipped MCP tools cover orientation, routing, impact, and search. `FileCard.git_intelligence` and `SymbolCard.last_change` (symbol-level granularity) are fully functional.

**Phase 3 — git intelligence and co-change (shipped).** Shipped: file-scoped history, hotspots, ownership, co-change via `gix`; inline `# DECISION:` marker parsing; DecisionCard; graph-level `CoChangesWith` edges.

**Phase 4 — card commentary tier (shipped).** LLM synthesis is trait-shaped (`CommentaryGenerator`), defaults to `NoOpGenerator`, opts into `ClaudeCommentaryGenerator` when `SYNREPO_ANTHROPIC_API_KEY` is set. Commentary lands in overlay, never in graph. Freshness labels (fresh / stale / missing / unsupported / invalid / budget_withheld) are enforced at the card surface.

**Phase 5 — overlay cross-linking (shipped).** Shipped: graph-distance plus prose-identifier candidate triage, two-stage Claude-backed LLM proposal, normalized fuzzy verification (3-stage cascade), confidence tiers, review queue, card surfacing at Deep tier, curated-mode promotion that creates `Governs` edges with `Epistemic::HumanDeclared`. The embedding model (`all-MiniLM-L6-v2` via `ort`) is fully integrated for semantic triage.

**Phase 6 — polish (shipped).** Shipped: `synrepo export`, `synrepo upgrade`, `synrepo status`, `synrepo agent-setup` across agent targets, and dedicated `synrepo compact` (with retention policies and repair-log rotation).

**Phase 7 — runtime UX (shipped).** The live-mode TUI dashboard hosted by foreground `synrepo watch` is shipped. The repair wizard UI (with `ratatui` + `crossterm`) is fully complete.

### Hidden Triumphs (Architectural Hard-Wins)

1. **The Write Admission Gate (`runtime-safety-hardening-v1`)**: A unified, thread-scoped write admission path that cleanly delegates to the watch daemon when active, making CLI/Daemon contention bulletproof.
2. **Parser CI Strictness (`parse-hardening-tree-sitter`)**: Tree-sitter degradation is locked down via CI. If a query misses a capture or pattern indexes drift, CI fails immediately.
3. **Agent Doctrine Consistency (`agent-doctrine-v1`)**: The agent doctrine is centralized into a single macro (`doctrine_block!()`). Every shim and MCP tool description is byte-identical at compile time, guaranteeing consistent escalation rules.

Each phase produces something shippable. Phase 0 is a faster grep. Phase 1 is a static graph. **Phase 2 is the product.** Phase 4 adds prose. Phase 5 adds cross-linking. A reader who loses faith in the LLM parts after phase 2 still has a useful tool.

---

## References

**Substrate and retrieval lineage**
- syntext — <https://github.com/whit3rabbit/syntext>
- syntext architecture — <https://github.com/whit3rabbit/syntext/blob/main/docs/ARCHITECTURE.md>
- Russ Cox, *Regular Expression Matching with a Trigram Index* — <https://swtch.com/~rsc/regexp/regexp4.html>
- Sourcegraph Zoekt — <https://github.com/sourcegraph/zoekt>
- *How GitHub Built Code Search* — <https://github.blog/engineering/architecture-optimization/how-we-built-github-code-search/>
- Cursor, *Fast Regex Search* — <https://cursor.com/blog/fast-regex-search>

**Influences**
- *Karpathy's LLM Knowledge Bases* writeup — <https://www.franksworld.com/2026/04/06/how-to-build-a-self-evolving-ai-memory-with-karpathys-llm-knowledge-bases/>
- repowise — <https://github.com/repowise-dev/repowise> (task-shaped MCP design, git intelligence)

**Tree-sitter ecosystem**
- Tree-sitter — <https://tree-sitter.github.io/tree-sitter>
- tree-sitter Rust crate — <https://docs.rs/tree-sitter>
- Per-language crates — `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-typescript`, etc.
- Fernando Ayats, *The tree-sitter packaging mess* — <https://ayats.org/blog/tree-sitter-packaging>

**Agent surface**
- Model Context Protocol — <https://modelcontextprotocol.io>

---

## The one-line summary

synrepo is a context compiler for AI coding agents: it precomputes a small, deterministic, queryable working set of structural facts about a software project and serves them as token-budgeted cards through an MCP server, so an agent can act like it already knows the codebase without stuffing huge file chunks into context. The graph holds only what parsers, git, and humans observed directly. LLM-proposed cross-links and commentary live in a separate overlay store, queryable but never authoritative. The wedge is fewer blind reads, fewer wrong-file edits, lower token burn, and faster orientation on unfamiliar code. The graph is infrastructure. Cards are the product.
