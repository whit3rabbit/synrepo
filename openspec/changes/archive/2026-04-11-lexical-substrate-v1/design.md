## Context

The current substrate path proves that synrepo can create `.synrepo/index/`, write config, and call `syntext`, but it does not yet define or implement the deterministic repository-walking and file-handling contract that the roadmap expects. `src/substrate/` carries the lexical substrate entry points, while `src/substrate/discover.rs` and `src/pipeline/structural/` document the intended Phase 0 behavior as TODOs or staging seams.

That is the right next planning step because the lexical substrate is the first real product layer after the planning foundation. If discovery, classification, and exact search are fuzzy, every later graph or bootstrap change inherits ambiguity.

## Goals / Non-Goals

**Goals:**
- Define implementable Phase 0 behavior for repository discovery, file classification, and exact lexical retrieval.
- Use the existing Rust stack already chosen in the repo: `ignore` for walking, `syntext` for indexing/search, and synrepo-owned classification rules.
- Lock the observable search behavior for `synrepo init` and `synrepo search`.
- Make ugly-repo behavior explicit enough to test, especially around redaction, encoding, file size, and skipped inputs.

**Non-Goals:**
- Implement the structural graph, tree-sitter parse pipeline, or git mining.
- Change card, MCP, or overlay behavior.
- Introduce new runtime stores beyond the existing `.synrepo/index/` Phase 0 path.
- Solve all Track L storage/versioning work in this change.

## Decisions

1. Discovery is a synrepo-owned phase, not an implicit side effect of `syntext`.
   synrepo should decide which files count, why others are skipped, and which file class each discovered artifact belongs to. `syntext` remains the indexing/search engine, not the policy owner.

2. Classification uses explicit support tiers.
   Supported code gets structural eligibility later.
   Indexed-only text remains searchable without symbol extraction.
   Markdown and notebooks receive special handling.
   Skipped files carry a concrete skip reason.

3. Ugly-repo rules are part of the contract.
   `.gitignore`, redaction globs, max file size, UTF-8 sniffing, LFS pointer detection, empty-file handling, and symlink protection should all be visible behavior, not implementation trivia.

4. Index behavior is deterministic and local.
   `synrepo init` builds the first substrate index after creating `.synrepo/`.
   `synrepo search` opens the existing index and reports exact lexical matches.
   If the index is missing or stale in a way the command can detect, the user should get a defined error or rebuild path rather than a vague failure.

5. Language support is explicit, not inferred from dependency presence alone.
   The first supported-code set is the languages already pinned in `Cargo.toml`: Rust, Python, TypeScript, and TSX. Other common text formats remain indexed-only until a later structural change gives them deeper support.

## Risks / Trade-offs

- Letting `syntext` own too much policy would be faster short-term, but it would make synrepo's repository contract implicit and hard to test.
- Discovery rules that are too strict risk skipping useful files; rules that are too loose will poison indexing with binaries, generated noise, or secrets. The contract should prefer explicit skip reasons over silent inclusion.
- This change intentionally stops short of the broader storage/migration policy. That remains a separate capability so the substrate change does not become a junk drawer.
