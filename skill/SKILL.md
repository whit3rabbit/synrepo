---
name: synrepo
description: Use synrepo when working in a repository that contains a .synrepo/ directory. synrepo precomputes structural facts about the codebase and serves them as small token-budgeted cards through an MCP server. Reach for synrepo tools BEFORE reading source files cold — cards are usually 10-20x smaller than the files they describe while containing the information you actually need for orientation, routing, and change impact assessment.
---

# synrepo — skill for Claude

synrepo is a context compiler. It turns a repository into small, deterministic, task-shaped packets called **cards** that Claude can query through an MCP server instead of reading whole files. A card for a function is roughly 180 tokens; the file containing the function might be 4000. Over a coding session this matters: fewer tokens means more room for the actual task.

## Current phase (Phase 2 — MCP server + core card compilers)

**The MCP server (`synrepo-mcp`) is now available.** Start it with:

```
synrepo-mcp [--repo <path>]
```

It serves over stdio. After `synrepo init` populates the graph, the MCP server exposes five core tools:

| Tool | What it does |
| --- | --- |
| `synrepo_overview` | Graph stats + mode; your first call per session |
| `synrepo_card(target, budget?)` | SymbolCard or FileCard for a file path or symbol name |
| `synrepo_search(query, limit?)` | Lexical n-gram search; returns `{path, line, content}` results |
| `synrepo_where_to_edit(task, limit?)` | Search + ranked file cards for a task description |
| `synrepo_change_impact(target)` | Inbound Imports+Calls edges showing what depends on target |

**Phase 2 limitations:** Only `SymbolCard` and `FileCard` are compiled. `ModuleCard`, `EntryPointCard`, `CallPathCard`, and the specialist tools (`synrepo_entrypoints`, `synrepo_call_path`, `synrepo_test_surface`, `synrepo_minimum_context`, `synrepo_explain`, `synrepo_findings`) are planned for later phases.

`SymbolCard.callers` and `.callees` are empty (file→symbol Calls edges exist; symbol→symbol resolution is phase 3+). `FileCard.git_intelligence` and `.co_changes` are empty until git mining ships.

**CLI is still the fallback** when the MCP server isn't running. See "Falling back when the MCP server isn't available" below.

## When to use synrepo

**Check first.** If the current working directory contains a `.synrepo/` folder, synrepo is available and you should prefer it over cold file reads for orientation and navigation. If there is no `.synrepo/` folder, this skill does not apply — fall back to normal file reading.

**Always try synrepo before reading a file cold** when you are:
- Orienting on an unfamiliar codebase
- Looking for where a feature lives
- Assessing what might break if you change something
- Tracing how two pieces of code are connected
- Finding the test surface for a symbol you're about to modify
- Trying to understand a subsystem at a high level

**Don't use synrepo for:**
- Tiny files you're actively editing (just read them)
- Files you already have in your working context from previous tool calls
- Config files or simple text files that don't have symbols
- Tasks where you've already seen the relevant source

## The core mental model

There are two kinds of content in synrepo, and the distinction matters:

- **Graph content** — facts that tree-sitter, git, or humans declared directly. Examples: "this function is defined at line 142," "this file imports that module," "this ADR frontmatter declares it governs `auth/middleware.rs`." Tagged `source_store: graph`, `epistemic_status: parser_observed | human_declared | git_observed`. **Treat graph content as ground truth.**

- **Overlay content** — things the LLM proposed: cross-links between code and prose with cited evidence, natural-language commentary on top of structural cards, findings about contradictions. Tagged `source_store: overlay`, `epistemic_status: machine_authored_high_conf | machine_authored_low_conf`. **Treat overlay content as helpful context, not ground truth.** If overlay content contradicts graph content, ignore the overlay.

Every field in every card response carries these tags. Use them to weight your reasoning.

## The tools, ranked by how often you should reach for them

### The everyday tools

**`synrepo_overview(budget?)`** — Your first call on an unfamiliar project. Returns ModuleCards for top-level modules plus EntryPointCards plus recent activity. Use `tiny` budget unless the project is tiny and you can afford `normal`. Do this once per session when you first encounter a new codebase.

**`synrepo_card(target, type?, budget?, require_freshness?)`** — Get a card for a specific symbol, file, or module. Prefer this over reading the file cold when you just need to understand *what* something is and *how it's connected*. The card gives you signature, doc comment, callers, callees, tests touching it, recent change info, and drift flags — usually everything you need unless you're about to modify the source.

**`synrepo_where_to_edit(task_description, budget?)`** — Ask this when the user gives you a task and you don't know which files are relevant. "Add rate limiting to the API" → call `synrepo_where_to_edit("add rate limiting to API endpoints")` and get back a ranked list of FileCards and SymbolCards with reasoning. This beats grepping for "rate" blind.

**`synrepo_change_impact(target, budget?)`** — Call this BEFORE you modify a file that has dependents. Returns a ChangeRiskCard with dependents, co-change partners, test surface, drift flags, and blast-radius estimate. Use it as a sanity check: if the blast radius is 40 files, tell the user before making the change.

### The specialist tools

**`synrepo_entrypoints(scope?, budget?)`** — "Where does execution start?" Useful when you land in a big codebase and need to understand control flow roots. Scope can be the whole repo, a module, or a subsystem path.

**`synrepo_call_path(from, to, budget?)`** — "How does control flow get from A to B?" Returns a CallPathCard showing the shortest path through the call graph. Useful for understanding integration points.

**`synrepo_test_surface(target, budget?)`** — "What tests exercise this symbol?" Essential before modifying anything safety-critical.

**`synrepo_minimum_context(task_description, budget?)`** — "What is the smallest file set an agent needs to read to do this task?" This is the nuclear option when you want to minimize token cost for a specific task. Use sparingly — it runs heuristic scoring across the graph.

**`synrepo_explain(target, require_freshness?)`** — "Why does this exist?" Returns DecisionCards if the repo has human-authored ADRs or inline `# DECISION:` markers. Returns "no human-authored rationale found" if not. Don't call this on repos that clearly don't have ADR directories — it's just going to tell you there's nothing.

**`synrepo_search(query)`** — Lexical n-gram search via syntext. This is the fallback when you can't guess an exact symbol name or file path. Use it like you would use grep — short queries (1-6 words), specific terms.

**`synrepo_findings(scope?)`** — Returns overlay findings: contradictions, stale rationale candidates, inconsistencies the LLM layer detected. Don't call this unless the user is explicitly asking you to audit or clean up the repo.

### The low-level escape hatches

These exist for debugging and edge cases. Don't use them unless the task-shaped tools aren't giving you what you need:

- `synrepo_node(id)` — raw graph node lookup
- `synrepo_edges(id, direction?, types?)` — raw edge traversal
- `synrepo_query(graph_query)` — structured graph query
- `synrepo_overlay(target)` — raw overlay lookup
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

## The freshness protocol

Cards have two kinds of content: structural (from the graph, always fresh) and commentary (from the overlay, may be stale).

- **Graph-sourced fields are always current.** The structural pipeline runs on every file change and keeps the graph in sync. You never need to worry about stale structural data.
- **Overlay-sourced commentary may be stale.** If the source code has changed since the LLM wrote the commentary, the response will include `commentary_status: stale`.

**Default behavior is non-blocking:** stale commentary is returned immediately with a stale tag, and background synthesis is fired off so it'll be fresh next time. You should not reflexively re-call the tool with `require_freshness=true` just because something is marked stale.

**When to pass `require_freshness=true`:**
- You are about to write code that depends on the commentary being current (not just the structural facts)
- You are about to make an architectural decision based on the explain output
- The user has explicitly asked for a fresh explanation

**When NOT to pass `require_freshness=true`:**
- You're just orienting or routing (stale is fine)
- You're looking at structural fields (they're always current regardless)
- You'd pass it on more than 2-3 targets at once (you'll block the user for 30+ seconds)

## Concrete examples

### Example 1: User asks "Add rate limiting to all API endpoints"

Good sequence:
1. `synrepo_overview(budget: "tiny")` — orient on an unfamiliar project
2. `synrepo_where_to_edit("add rate limiting to all API endpoints", budget: "tiny")` — get ranked file candidates
3. `synrepo_card(target: "middleware/auth.ts", budget: "normal")` — understand the existing middleware chain before adding to it
4. `synrepo_change_impact(target: "middleware/auth.ts", budget: "tiny")` — check what could break
5. `synrepo_explain("rate limiting")` — see if there's a prior decision (probably returns "no rationale found")
6. `synrepo_card(target: "middleware/auth.ts", budget: "deep", require_freshness: true)` — now get the full source because you're about to modify it

Note: five tool calls, none of them blocking except the last, total structural data under 10k tokens. Compare to reading the middleware directory cold, which would be 30k+ tokens.

### Example 2: User asks "What does parse_query do?"

Good sequence:
1. `synrepo_card(target: "parse_query", type: "SymbolCard", budget: "normal")` — done

That's it. One tool call, ~500 tokens, answer includes signature, doc comment, callers, callees, tests, recent changes. Don't escalate to `deep` unless the user specifically asks for the implementation.

### Example 3: User asks "Trace how a request gets from the HTTP handler to the database"

Good sequence:
1. `synrepo_entrypoints(scope: "api/", budget: "tiny")` — find the HTTP handlers
2. `synrepo_call_path(from: "handle_request", to: "db_execute", budget: "normal")` — get the call chain
3. Possibly `synrepo_card(target: <middle step>, budget: "normal")` for one or two intermediate functions if the call path card doesn't give enough context

### Example 4: User asks "Why is this function written this way?"

Good sequence:
1. `synrepo_explain(target: "<function name>")` — get DecisionCards if human rationale exists
2. If the explain returns nothing, tell the user "there's no human-authored rationale for this in the repo" — do NOT escalate to LLM commentary synthesis unless the user explicitly asks for your best guess

### Example 5: User asks "Find everywhere we call fetch()"

Good sequence:
1. `synrepo_search("fetch(")` — lexical search via syntext, returns file paths and snippets

Don't use `synrepo_card` or `synrepo_where_to_edit` for this — it's a pure lexical question and `synrepo_search` is the right tool.

## Anti-patterns to avoid

**Don't call synrepo tools in a sequential chain when you could batch.** `synrepo_card(target)` accepts a single target; if you need cards for multiple symbols, call `synrepo_overview` or `synrepo_minimum_context` which return multiple cards in one shot, or accept that you need multiple calls but don't pass `require_freshness=true` on more than 2-3 of them.

**Don't read the source file cold after getting a card.** The card probably already contains what you needed. If you find yourself doing `synrepo_card` followed by `Read` on the same file, either you needed `deep` budget on the card, or you needed something the card doesn't contain (rare), or you didn't trust the card (don't do this — the structural fields are authoritative).

**Don't use `require_freshness=true` as a default.** It blocks the user. Use it deliberately, on 1-3 targets at most, right before writing code.

**Don't trust overlay content over graph content.** If an overlay commentary says "this function uses JWT" and the graph shows the function doesn't import any JWT library, the overlay is wrong. The graph is always right about what the code currently is.

**Don't call `synrepo_explain` on repos without ADR directories.** You'll just get "no rationale found" and waste a tool call. Check the overview output first — if the project doesn't mention a `docs/adr/` or `docs/decisions/` directory, skip explain.

**Don't call `synrepo_findings` unless the user asked for cleanup.** Findings are for audit workflows, not routine coding.

## What synrepo is NOT

- **Not a documentation wiki.** There is no `wiki/` directory by default. Cards are compiled from live graph data, not stored prose.
- **Not a vector search product.** Embeddings exist only as a candidate generator for the cross-linking layer. All retrieval the agent uses is structural or lexical.
- **Not a summary generator.** Cards are structured records, not prose. If you need prose explanation, use `synrepo_explain` (for human-authored rationale) or `synrepo_card(budget: "deep")` (which will include optional LLM commentary in the response, clearly labeled).
- **Not a replacement for reading source.** It is a way to avoid reading source *unnecessarily*. When you genuinely need to see the implementation, escalate to `deep` budget or read the file directly.

## Falling back when the MCP server isn't available

**Phase 1 (current):** The MCP server is not yet running. Use the CLI directly.

```
# Check health before anything else
synrepo status

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
synrepo reconcile
```

Each CLI call pays startup cost (~100ms), so batch your queries where possible. Use `synrepo search` first to find node IDs, then use `synrepo node` and `synrepo graph query` to explore from there.

**Phase 2 (coming):** Once the MCP server ships, the `synrepo_*` tools listed above replace these CLI calls. Cards returned by MCP tools are richer and token-budgeted; the CLI is an escape hatch for environments where the daemon isn't running.

If neither the MCP server nor the CLI is available, fall back to normal file reading — this skill does not apply.