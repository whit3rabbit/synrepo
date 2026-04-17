use super::super::run_structural_compile;
use super::support::open_graph;
use crate::{
    config::Config,
    core::ids::NodeId,
    structure::graph::{EdgeKind, GraphStore},
};
use std::fs;
use tempfile::tempdir;

#[test]
fn stage4_emits_calls_edge_for_cross_file_rust_call() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/utils.rs"), "pub fn helper() {}\n").unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "mod utils;\nfn entry() { utils::helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let utils_file = graph.file_by_path("src/utils.rs").unwrap().unwrap();
    let helper_sym_id = graph
        .outbound(NodeId::File(utils_file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .find_map(|e| match e.to {
            NodeId::Symbol(id) => Some(id),
            _ => None,
        })
        .expect("helper symbol must exist in graph");

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(lib_file.id), Some(EdgeKind::Calls))
        .unwrap();

    assert!(
        calls.iter().any(|e| e.to == NodeId::Symbol(helper_sym_id)),
        "expected Calls edge from src/lib.rs to helper symbol; got: {calls:?}"
    );
}

#[test]
fn stage4_emits_imports_edge_for_typescript_relative_import() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/utils.ts"),
        "export function helper() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { helper } from './utils';\nhelper();\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let utils_file = graph.file_by_path("src/utils.ts").unwrap().unwrap();

    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();

    assert!(
        imports.iter().any(|e| e.to == NodeId::File(utils_file.id)),
        "expected Imports edge from src/main.ts to src/utils.ts; got: {imports:?}"
    );
}

// ── parse-hardening-tree-sitter §7: stage-4 contract pins ────────────────────

/// 7.1: an ambiguous call name emits `Calls` edges to every matching candidate.
#[test]
fn stage4_ambiguous_call_name_fans_out_to_all_candidates() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Two symbols with the same short name `helper` in different files.
    // Contents must differ so FileNodeId derivation (content-hash) gives each
    // file its own id — identical content would collapse into stage-6 rename.
    fs::write(
        repo.path().join("src/a.rs"),
        "// a\npub fn helper() -> i32 { 1 }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "// b\npub fn helper() -> i32 { 2 }\n",
    )
    .unwrap();
    fs::write(repo.path().join("src/main.rs"), "fn main() { helper(); }\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    let main_file = graph
        .file_by_path("src/main.rs")
        .unwrap()
        .unwrap_or_else(|| panic!("src/main.rs missing; graph has: {paths:?}"));
    let a_file = graph
        .file_by_path("src/a.rs")
        .unwrap()
        .unwrap_or_else(|| panic!("src/a.rs missing; graph has: {paths:?}"));
    let b_file = graph
        .file_by_path("src/b.rs")
        .unwrap()
        .unwrap_or_else(|| panic!("src/b.rs missing; graph has: {paths:?}"));

    let a_helper = graph
        .outbound(NodeId::File(a_file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .find_map(|e| match e.to {
            NodeId::Symbol(id) => Some(id),
            _ => None,
        })
        .expect("a::helper must be defined");
    let b_helper = graph
        .outbound(NodeId::File(b_file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .find_map(|e| match e.to {
            NodeId::Symbol(id) => Some(id),
            _ => None,
        })
        .expect("b::helper must be defined");

    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();

    assert!(
        calls.iter().any(|e| e.to == NodeId::Symbol(a_helper)),
        "expected Calls edge from main.rs to a::helper; got: {calls:?}"
    );
    assert!(
        calls.iter().any(|e| e.to == NodeId::Symbol(b_helper)),
        "expected Calls edge from main.rs to b::helper; got: {calls:?}"
    );
}

/// 7.2: unresolved call or import is skipped silently, not an error.
#[test]
fn stage4_unresolved_call_or_import_is_skipped_without_error() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Call to a nonexistent function + import of a missing relative module.
    fs::write(
        repo.path().join("src/main.ts"),
        "import { gone } from './does_not_exist';\nnonexistent_fn();\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    // Must not return Err.
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();

    assert!(
        calls.is_empty(),
        "unresolved call must not emit Calls edge; got: {calls:?}"
    );
    assert!(
        imports.is_empty(),
        "unresolved import must not emit Imports edge; got: {imports:?}"
    );
}

/// 7.3: TSX relative imports resolve the same way as TypeScript relative imports.
#[test]
fn stage4_emits_imports_edge_for_tsx_relative_import() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/card.tsx"),
        "export function Card() { return null; }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/app.tsx"),
        "import { Card } from './card';\nexport function App() { return <Card />; }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let app_file = graph.file_by_path("src/app.tsx").unwrap().unwrap();
    let card_file = graph.file_by_path("src/card.tsx").unwrap().unwrap();

    let imports = graph
        .outbound(NodeId::File(app_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(card_file.id)),
        "expected TSX Imports edge from src/app.tsx to src/card.tsx; got: {imports:?}"
    );
}

/// 7.4: Python dotted imports resolve to `a/b.py`.
#[test]
fn stage4_emits_imports_edge_for_python_dotted_import() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("pkg/sub")).unwrap();

    fs::write(repo.path().join("pkg/sub/__init__.py"), "").unwrap();
    fs::write(repo.path().join("pkg/__init__.py"), "").unwrap();
    fs::write(repo.path().join("pkg/sub/mod.py"), "def f(): pass\n").unwrap();
    fs::write(
        repo.path().join("main.py"),
        "import pkg.sub.mod\npkg.sub.mod.f()\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("main.py").unwrap().unwrap();
    let mod_file = graph.file_by_path("pkg/sub/mod.py").unwrap().unwrap();

    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(mod_file.id)),
        "expected Python dotted Imports edge from main.py to pkg/sub/mod.py; got: {imports:?}"
    );
}

/// 7.5: Rust `use` last-name captures an identifier but stage 4 has no path
/// → file mapping for Rust, so no Imports edge is emitted. This is an
/// intentional phase-1 boundary (stage 4 only resolves TS/Python imports);
/// see `resolve_import_ref` in `stage4.rs`. Update this test when Rust
/// import resolution is promoted out of phase 1.
#[test]
fn stage4_rust_use_last_name_is_not_resolved_to_file() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/util.rs"), "pub fn helper() {}\n").unwrap();
    fs::write(
        repo.path().join("src/main.rs"),
        "use crate::util::helper;\nfn main() {}\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.rs").unwrap().unwrap();
    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();

    assert!(
        imports.is_empty(),
        "Rust `use` last-name must not produce an Imports edge in phase 1; got: {imports:?}"
    );
}
