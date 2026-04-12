## Context

`ExtractedSymbol.signature` and `ExtractedSymbol.doc_comment` are `Option<String>` fields in `src/structure/parse/mod.rs`. They propagate through `SymbolNode` to `GraphCardCompiler`, which already reads them and applies budget-tier truncation logic. The only missing piece is the extraction: both fields are hardcoded to `None` at `src/structure/parse/extract.rs:108-109`.

The existing extraction runs one `tree_sitter::QueryCursor` pass over each file using `language.definition_query()`. Each match yields an `@item` node (the full declaration) and a `@name` node (the identifier). We have direct access to each matched `@item` node after the match.

Additionally, `src/surface/card/compiler.rs` is at 420 lines and must be split before adding snapshot test fixtures.

## Goals / Non-Goals

**Goals:**
- Populate `signature` and `doc_comment` on all matched `SymbolNode`s for Rust, Python, and TypeScript/TSX
- Keep extraction stateless: no new parse passes, no additional I/O per file
- Split `compiler.rs` into sub-modules to bring it within the 400-line limit

**Non-Goals:**
- Extracting cross-reference doc links (e.g., `[`TypeName`]` in Rust)
- Synthesis or inference for undocumented symbols
- Full multi-line signature normalization (collapse to one line; do not parse parameter types)
- Languages beyond Rust, Python, TypeScript/TSX in this change

## Decisions

### Decision: Walk the matched `@item` node rather than adding separate queries

**Chosen:** After the definition query loop produces each `@item` node, walk the item node's siblings and children in the already-parsed AST to extract doc comment and signature.

**Alternative considered:** Add `doc_comment_query()` and `signature_query()` methods to `Language` and run them as a second `QueryCursor` pass over the file, correlating results to matched symbols by byte offset.

**Rationale:** We already hold the `@item` node from the definition match. Walking its context is O(adjacent siblings) per symbol — far cheaper than a second full-file scan, and it keeps all per-symbol extraction logic colocated. The second-pass approach requires byte-range deduplication and risks diverging from the definition query's node selection logic.

### Decision: Extract signature as the item's declaration text, stripped of body

**Chosen:** Take the text from the start of the `@item` node to the first `{` (for block-bodied items) or the end of the declaration line (for type aliases, constants, etc.). Collapse internal newlines and excess whitespace to a single line.

**Extraction per language:**

| Language | Signature range | Body delimiter |
|----------|-----------------|----------------|
| Rust | Start of `@item` to first `{` or `;` | `{` or `;` |
| Python | `def`/`class` declaration to the `:` that ends the header | `:` (not inside parens/brackets) |
| TypeScript/TSX | Start of function/class/method to first `{` | `{` |

For items with no body delimiter (e.g., a Rust `type Foo = Bar;` before the `;`), the full declaration line is the signature.

### Decision: Extract doc comment by walking prior siblings in the AST

**Chosen:** For each `@item` node, walk backwards through its preceding siblings in the parent's child list. Collect adjacent comment nodes up to the first non-comment, non-whitespace sibling. Join and strip comment markers.

**Comment node names per grammar:**

| Language | Doc comment node | Block comment node |
|----------|------------------|--------------------|
| Rust | `line_comment` (starts with `///`) | `block_comment` (starts with `/**`) |
| Python | `expression_statement` containing a `string` node as first body child | — |
| TypeScript/TSX | `comment` (starts with `//` or `/**`) | `comment` |

For Python, the docstring is the first statement inside the body node (not a preceding sibling). Walk into the body node's first child and check if it is a bare `string` literal.

**Decorator handling (Python):** `decorator` nodes may appear between the preceding comment and the `def`/`class` keyword. Skip decorators when walking backwards to find the doc comment.

### Decision: Split `compiler.rs` into sub-modules now

Snapshot tests require fixture code that would push `compiler.rs` further over the 400-line limit. Split before adding tests.

**Sub-module layout:**
- `compiler/mod.rs` — `GraphCardCompiler` struct, `CardCompiler` impl (delegates to sub-modules)
- `compiler/symbol.rs` — `symbol_card()`, `estimate_tokens_symbol()`
- `compiler/file.rs` — `file_card()`, `estimate_tokens_file()`
- `compiler/resolve.rs` — `resolve_target()`
- `compiler/io.rs` — `read_symbol_body()`
- `compiler/tests.rs` — existing integration test suite, new SymbolCard snapshot tests

## Risks / Trade-offs

**Node name variance across tree-sitter grammars** → Mitigation: verify comment and declaration node names against the actual grammar at parse time in a unit test. Add a test fixture file per language with a documented function to assert non-None extraction.

**Multi-line Rust signatures** (common for functions with many parameters): collecting to first `{` may include newlines. → Mitigation: collapse consecutive whitespace to single spaces after extraction; cap signature at 200 characters with a trailing `…` if over.

**Python `@property` and chained decorators** between the preceding comment and the `def`: the backwards sibling walk may land on a decorator and stop. → Mitigation: explicitly skip `decorator` nodes when scanning backwards for a doc comment.

**TypeScript arrow functions assigned to `const`** (`const foo = () => ...`): the `@item` is the `variable_declaration`, not a function declaration. Signature extraction may include the full RHS. → Mitigation: for arrow-function variable declarations, take only the LHS up to `=` as the signature.
