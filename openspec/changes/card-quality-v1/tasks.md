## 1. Split compiler.rs into sub-modules

- [ ] 1.1 Create `src/surface/card/compiler/` directory and move existing `compiler.rs` content into `compiler/mod.rs`
- [ ] 1.2 Extract `symbol_card()` and `estimate_tokens_symbol()` into `compiler/symbol.rs`; re-export via `mod.rs`
- [ ] 1.3 Extract `file_card()` and `estimate_tokens_file()` into `compiler/file.rs`; re-export via `mod.rs`
- [ ] 1.4 Extract `resolve_target()` into `compiler/resolve.rs`; re-export via `mod.rs`
- [ ] 1.5 Extract `read_symbol_body()` into `compiler/io.rs`; re-export via `mod.rs`
- [ ] 1.6 Move existing tests into `compiler/tests.rs` with `#[cfg(test)]`
- [ ] 1.7 Verify `cargo test -p synrepo` passes and all modules are under 400 lines

## 2. Add doc comment extraction to structural parse

- [ ] 2.1 In `src/structure/parse/extract.rs`, add helper `extract_doc_comment(node: Node, source: &[u8], language: Language) -> Option<String>` that walks the node's preceding siblings and collects adjacent comment nodes
- [ ] 2.2 Implement Rust branch: collect contiguous `line_comment` siblings that start with `///`; strip `/// ` prefix; join with newlines
- [ ] 2.3 Implement Python branch: inspect the body node's first child for a bare `expression_statement` containing a `string`; return its content with enclosing quotes stripped
- [ ] 2.4 Implement TypeScript/TSX branch: collect the nearest preceding `comment` sibling starting with `/**`; strip `/** */` markers
- [ ] 2.5 Wire `extract_doc_comment()` into the definition query loop so `ExtractedSymbol.doc_comment` is populated

## 3. Add signature extraction to structural parse

- [ ] 3.1 In `src/structure/parse/extract.rs`, add helper `extract_signature(node: Node, source: &[u8], language: Language) -> Option<String>` 
- [ ] 3.2 Implement Rust branch: take node text from start to first `{` or `;`; collapse internal whitespace; cap at 200 chars with `…`
- [ ] 3.3 Implement Python branch: take node text from start to the `:` that ends the header (tracking paren/bracket depth); collapse internal whitespace
- [ ] 3.4 Implement TypeScript/TSX branch: take node text from start to first `{`; handle arrow-function variables (`const foo = ...`) by taking only LHS up to `=`
- [ ] 3.5 Wire `extract_signature()` into the definition query loop so `ExtractedSymbol.signature` is populated

## 4. Add unit tests for extraction helpers

- [ ] 4.1 Add Rust fixture (a `///`-documented `pub fn` with parameters) to the parse tests in `src/structure/parse/tests.rs` and assert non-None `signature` and `doc_comment`
- [ ] 4.2 Add Python fixture (a `"""docstring"""` function) and assert extraction
- [ ] 4.3 Add TypeScript fixture (a `/** JSDoc */` function) and assert extraction
- [ ] 4.4 Add a no-doc fixture per language and assert `doc_comment: None`, `signature: Some(_)`

## 5. Add SymbolCard snapshot tests

- [ ] 5.1 Add a test in `src/surface/card/compiler/tests.rs` (or a new `src/surface/card/snapshots/` file) that builds a `SymbolNode` with populated `signature` and `doc_comment` and calls `symbol_card()` at all three budget tiers
- [ ] 5.2 Run `cargo test` once to generate initial insta snapshots; review and accept with `cargo insta review`
- [ ] 5.3 Verify snapshot files are committed alongside test code

## 6. Verify and close

- [ ] 6.1 Run `make check` (fmt + clippy + tests); fix any warnings
- [ ] 6.2 Confirm all `src/**/*.rs` files are under 400 lines
- [ ] 6.3 Run `cargo run -- init` on the synrepo repo itself and `cargo run -- node <any_symbol_id>` to verify `signature` and `doc_comment` appear in output for at least one Rust symbol
