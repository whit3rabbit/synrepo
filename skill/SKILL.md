---
name: synrepo
description: Use synrepo when working in a repository that contains a .synrepo/ directory. synrepo precomputes structural facts about the codebase and serves them as small token-budgeted cards through an MCP server. Reach for synrepo tools BEFORE reading source files cold. Cards are usually 10-20x smaller than the files they describe while containing the information you actually need for orientation, routing, and first-pass change impact assessment.
---

# synrepo — skill for Claude

synrepo is a context compiler. It turns a repository into small, deterministic, task-shaped packets called **cards** that Claude can query through an MCP server instead of reading whole files. A card for a function is roughly 180 tokens; the file containing the function might be 4000. Over a coding session this matters: fewer tokens means more room for the actual task.

## Current surface (Milestone 3 + Milestone 4 repair loop)

**The MCP server (`synrepo-mcp`) is now available.** Start it with:

```
synrepo-mcp [--repo <path>]
```

It serves over stdio. After `synrepo init` populates the graph, the MCP server exposes six core tools:

| Tool | What it does |
| --- | --- |
| `synrepo_overview` | Graph stats + mode; your first call per session |
| `synrepo_card(target, budget?)` | SymbolCard or FileCard for a file path or symbol name |
| `synrepo_search(query, limit?)` | Lexical n-gram search; returns `{path, line, content}` results |
| `synrepo_where_to_edit(task, limit?)` | Search + ranked file cards for a task description |
| `synrepo_change_impact(target)` | Approximate file-level inbound Imports+Calls edges showing what depends on target |
| `synrepo_entrypoints(scope?, budget?)` | Heuristic entry-point detection: binaries, CLI commands, HTTP handlers, library roots |
| `synrepo_module_card(path, budget?)` | Directory-level summary: files, nested modules, public symbol count |
| `synrepo_public_api(path, budget?)` | Public API surface of a directory: exported symbols with kinds and signatures, public entry points, recently changed API at deep budget (Rust-only visibility detection in v1) |
| `synrepo_minimum_context(target, budget?)` | Budget-bounded 1-hop neighborhood around a focal symbol or file: structural neighbors, governing decisions, co-change partners |
| `synrepo_recent_activity(limit?, kinds?)` | Bounded operational event history (default limit 20) |
| `synrepo_findings(node_id?)` | Machine-authored cross-link candidates with provenance and tier |

**Current limitations:** `SymbolCard.callers` and `.callees` are empty — the graph emits file→symbol `Calls` edges, not symbol→symbol. Specialist tools (`synrepo_call_path`, `synrepo_test_surface`, `synrepo_explain`) are not yet implemented. `synrepo_public_api` visibility detection is Rust-specific; non-Rust directories return empty symbol lists.

`SymbolCard.callers` and `.callees` are empty because the current graph emits file→symbol `Calls` edges, not symbol→symbol call edges. `synrepo_change_impact` is therefore a first-pass routing aid built from inbound `Imports` edges plus file→symbol `Calls` edges; overloaded names can produce false positives.

**CLI is still the fallback** when the MCP server isn't running. See "Falling back when the MCP server isn't available" below.

## When to use synrepo

**Check first.** If the current working directory contains a `.synrepo/` folder, synrepo is available and you should prefer it over cold file reads for orientation and navigation. If there is no `.synrepo/` folder, this skill does not apply — fall back to normal file reading.

**Always try synrepo before reading a file cold** when you are:
- Orienting on an unfamiliar codebase
- Looking for where a feature lives
- Getting a first-pass view of what might break if you change something
- Tracing how two pieces of code are connected at a high level
- Finding the test surface for a symbol you're about to modify
- Trying to understand a subsystem at a high level

**Don't use synrepo for:**
- Tiny files you're actively editing (just read them)
- Files you already have in your working context from previous tool calls
- Config files or simple text files that don't have symbols
- Tasks where you've already seen the relevant source

## The core mental model

There are two kinds of content in synrepo, and the distinction matters:

- **Graph content** — facts that tree-sitter, git, or humans declared directly. Examples: "this function is defined at line 142," "this file imports that module," "this ADR frontmatter declares it governs `auth/middleware.rs`." Tagged `source_store: graph`, `epistemic_status: parser_observed | human_declared | git_observed`. **Treat graph content as the primary source of truth, but remember some current relationship surfaces are still approximate, especially change-impact hints built from file→symbol call resolution.**

- **Overlay content** — things the LLM proposed: cross-links between code and prose with cited evidence, natural-language commentary on top of structural cards, findings about contradictions. Tagged `source_store: overlay`, `epistemic_status: machine_authored_high_conf | machine_authored_low_conf`. **Treat overlay content as helpful context, not ground truth.** If overlay content contradicts graph content, ignore the overlay.

The current shipped MCP surface is graph-first. Overlay-specific behavior is still a later phase, so treat overlay discussion here as architecture, not as a live tool contract.

## The tools, ranked by how often you should reach for them

### The everyday tools

**`synrepo_overview()`** — Your first call on an unfamiliar project. Returns repository mode plus graph counts and edge counts. Use it once per session to confirm what kind of graph is available before drilling in.

**`synrepo_card(target, budget?)`** — Get a card for a specific symbol or file. Prefer this over reading the file cold when you need to understand what something is and how it is connected. Today this returns `SymbolCard` or `FileCard`.

**`synrepo_where_to_edit(task, limit?)`** — Ask this when the user gives you a task and you do not know which files are relevant. It returns a small set of `FileCard` suggestions derived from lexical matches plus graph lookup.

**`synrepo_change_impact(target)`** — Call this before you modify a file or symbol that may have dependents. Today it returns a first-pass list of impacted files discovered through inbound `Imports` and file→symbol `Calls` edges, plus a total count. Treat it as orientation and routing help, not exact blast-radius proof.

**`synrepo_search(query, limit?)`** — Lexical n-gram search via syntext. This is the fallback when you cannot guess an exact symbol name or file path. Use it like grep: short queries, specific terms.

### Planned later tools

These are not part of the current MCP surface: `synrepo_call_path`, `synrepo_test_surface`, and `synrepo_explain`.

### The low-level escape hatches

These exist for debugging and edge cases. Use the CLI equivalents when the five MCP tools are not enough:

- `synrepo_node(id)` — raw graph node lookup
- `synrepo_edges(id, direction?, types?)` — raw edge traversal
- `synrepo_query(graph_query)` — structured graph query
- `synrepo_provenance(id)` — full provenance chain for debugging where a fact came from

## The budget protocol

Every card-returning tool takes a `budget` parameter. Default is `tiny`. You should almost always start with `tiny` and escalate only when you need more.

| Budget | Roughly | When to use |
| --- | --- | --- |
| `tiny` (default) | ~200 tokens per card, ~1k total | Orientation, routing, "which files matter," "is this the right file" |
| `normal` | ~500 per card, ~3k total | You've narrowed in on a specific symbol and need to understand its interface and behavior |
| `deep` | ~2k per card, ~10k total | You are about to write code that depends on the exact source, or you need the full text of a function body |

**Rule of thumb:** `tiny` to find, `normal` to understand, `deep` to write.

If you find yourself repeatedly escalating from `tiny` to `normal` on the same targets, the `tiny` tier wasn't what you needed — adjust and request `normal` directly next time in the session.

## Budget Escalation

The three budget tiers are a deliberate three-step interaction pattern, not a size knob. Always start at `tiny` or `normal` to orient, then escalate only when a specific field forces it.

**Decision rule:**

1. Start with `tiny` for any unfamiliar symbol or file. The response includes name, location, edge counts, and co-change state — enough to decide relevance.
2. Escalate to `normal` when you have confirmed relevance and need the interface: signature, doc comment, neighbor summaries, co-change partners.
3. Escalate to `deep` only when you must read or write the implementation. `deep` adds source body, full overlay commentary, and full neighbor cards.

**Do not default to `deep`.** Deep reads consume 10-15x more tokens than `tiny`. A pattern of `synrepo_card(target, budget: "deep")` on first contact is wasteful; use `tiny` first.

**Budget is preserved in every response.** Each card and neighborhood response includes a `budget` field so you can confirm which tier was served without inspecting field presence.

## Freshness today

The current MCP surface is graph-backed. There is no shipped `require_freshness` parameter, no commentary refresh flow, and no stable overlay-facing MCP tool contract yet.

- **Graph-sourced fields are the current truth.** The server reads structural data from the graph, while some planned card enrichments are not populated yet.
- **Overlay behavior is future architecture, not a live interface.** Do not plan workflows around stale commentary or blocking freshness overrides yet.

## Concrete examples

### Example 1: User asks "Add rate limiting to all API endpoints"

Good sequence:
1. `synrepo_overview()` — orient on an unfamiliar project
2. `synrepo_where_to_edit(task: "add rate limiting to all API endpoints", limit: 5)` — get ranked file candidates
3. `synrepo_card(target: "middleware/auth.ts", budget: "normal")` — understand the existing middleware chain before adding to it
4. `synrepo_change_impact(target: "middleware/auth.ts")` — get a first-pass blast radius before opening files
5. `synrepo_card(target: "middleware/auth.ts", budget: "deep")` — now get the full source because you're about to modify it

Note: this stays within the shipped five-tool surface and gets you to the real source only after narrowing the target.

### Example 2: User asks "What does parse_query do?"

Good sequence:
1. `synrepo_card(target: "parse_query", budget: "normal")` — done

That's it. One tool call, ~500 tokens, answer includes the current structural fields that are actually compiled, usually signature, doc comment, and identity/location. Don't escalate to `deep` unless the user specifically asks for the implementation.

### Example 3: User asks "Find everywhere we call fetch()"

Good sequence:
1. `synrepo_search("fetch(")` — lexical search via syntext, returns file paths and snippets

Don't use `synrepo_card` or `synrepo_where_to_edit` for this — it's a pure lexical question and `synrepo_search` is the right tool.

## Anti-patterns to avoid

**Don't invent tools that are not shipped.** The current MCP surface is exactly five tools. If you need entrypoints, call paths, tests, or rationale, fall back to CLI graph inspection and source reads instead of assuming a specialist tool exists.

**Don't read the source file cold after getting a card.** The card probably already contains what you needed. If you find yourself doing `synrepo_card` followed by `Read` on the same file, either you needed `deep` budget on the card, or you needed something the card doesn't contain (rare), or you needed exact proof beyond the current card surface.

**Don't trust overlay content over graph content.** If an overlay commentary says "this function uses JWT" and the graph shows the function doesn't import any JWT library, the overlay is wrong. The graph is always right about what the code currently is.

## What synrepo is NOT

- **Not a documentation wiki.** There is no `wiki/` directory by default. Cards are compiled from live graph data, not stored prose.
- **Not a vector search product.** Embeddings exist only as a candidate generator for the cross-linking layer. All retrieval the agent uses is structural or lexical.
- **Not a summary generator.** Cards are structured records, not prose. If you need prose explanation, that is still a later surface.
- **Not a replacement for reading source.** It is a way to avoid reading source *unnecessarily*. When you genuinely need to see the implementation, escalate to `deep` budget or read the file directly.

## Health checking and repair

Two new commands are available for detecting and repairing stale surfaces:

**`synrepo check [--json]`** — Read-only drift report. Runs without acquiring the writer lock. Each named surface (storage, graph currency, writer lock, declared links, overlay gaps) gets one finding with a drift class, severity, and recommended action. Use this in CI to detect drift without mutating state. Exits non-zero when actionable or blocked findings are present.

```
synrepo check          # human-readable report
synrepo check --json   # machine-readable JSON (one RepairReport object)
```

**`synrepo sync [--json]`** — Targeted repair. Runs storage maintenance for stale stores and a structural reconcile for stale graph state. Report-only and unsupported findings (overlay, exports, stale rationale) are surfaced but left untouched. Appends one JSONL line to `.synrepo/state/repair-log.jsonl` after each run.

```
synrepo sync           # repair + human-readable summary
synrepo sync --json    # repair + JSON summary
```

The resolution log at `.synrepo/state/repair-log.jsonl` records every sync run: timestamp, scope, findings, actions taken, and outcome. Use it to audit what was detected and repaired across a session or CI run.

## Falling back when the MCP server isn't available

If the MCP server is not running in your environment, use the CLI directly.

```
# Check health before anything else
synrepo status
synrepo check          # drift report across all repair surfaces

# Find a symbol or file name
synrepo search "parse_query"

# Dump a node's metadata (replace the ID with output from search or graph stats)
synrepo node symbol_0000000000000024

# See what depends on a node (outbound = what this node depends on)
synrepo graph query "inbound symbol_0000000000000024"
synrepo graph query "outbound symbol_0000000000000024 defines"

# See overall graph counts
synrepo graph stats

# Refresh the graph if sources changed
synrepo reconcile      # one-shot structural refresh
synrepo sync           # targeted repair (maintenance + reconcile as needed)
```

Each CLI call pays startup cost (~100ms), so batch your queries where possible. Use `synrepo search` first to find node IDs, then use `synrepo node` and `synrepo graph query` to explore from there.

The `synrepo_*` tools listed above are the current preferred interface when the MCP server is running. The CLI is the escape hatch for environments where it is not.

If neither the MCP server nor the CLI is available, fall back to normal file reading — this skill does not apply.
