## 0. Stabilize the current runtime baseline

- [x] 0.1 Fix tree-sitter 0.26 language-loader compatibility so the current crate builds again before substrate work continues

## 1. Define repository discovery and classification

- [x] 1.1 Implement discovery with `ignore::WalkBuilder` so configured roots respect `.gitignore`, repo excludes, and synrepo redaction globs
- [x] 1.2 Implement deterministic file classification for supported code, indexed-only text, markdown, notebooks, and skipped files with explicit skip reasons
- [x] 1.3 Add tests for file classification, redaction, size caps, empty files, LFS pointers, and unsupported encodings

## 2. Implement substrate index behavior

- [x] 2.1 Wire the substrate build path so `synrepo init` creates and populates `.synrepo/index/` using the declared discovery rules
- [x] 2.2 Define and implement search-time behavior for opening the index, reporting exact matches, and failing clearly when the substrate state is unusable
- [x] 2.3 Add tests for index build and exact lexical search over representative mixed-content fixtures

## 3. Tighten contract and command behavior

- [x] 3.1 Align the CLI and substrate code comments with the new discovery and search contract
- [x] 3.2 Confirm the first supported-code language set and indexed-only file classes match the durable substrate spec
- [x] 3.3 Validate the change with `openspec validate lexical-substrate-v1 --strict --type change`
