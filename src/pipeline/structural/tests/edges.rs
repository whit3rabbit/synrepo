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

// ── Rust `use` resolution ────────────────────────────────────────────────────

fn write_minimal_cargo_toml(repo: &std::path::Path, name: &str) {
    fs::write(
        repo.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n"
        ),
    )
    .unwrap();
}

/// Rust `use crate::a::A;` resolves to an Imports edge pointing at `src/a.rs`.
#[test]
fn stage4_rust_crate_prefix_resolves_to_module_file() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    write_minimal_cargo_toml(repo.path(), "stage4_rust_crate");

    fs::write(repo.path().join("src/a.rs"), "// a\npub struct A;\n").unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "// b\nuse crate::a::A;\npub fn use_a() -> A { A }\n",
    )
    .unwrap();
    // A lib.rs so the crate parses as a library — not strictly required for
    // file discovery but keeps the fixture looking like a real crate.
    fs::write(repo.path().join("src/lib.rs"), "pub mod a;\npub mod b;\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let b_file = graph.file_by_path("src/b.rs").unwrap().unwrap();
    let a_file = graph.file_by_path("src/a.rs").unwrap().unwrap();
    let imports = graph
        .outbound(NodeId::File(b_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(a_file.id)),
        "expected Imports edge from src/b.rs to src/a.rs; got: {imports:?}"
    );
}

/// Nested `src/foo/mod.rs` with `src/foo/bar.rs`: `use crate::foo::bar::Thing`
/// emits an edge to `src/foo/bar.rs` (longest-matching module file).
#[test]
fn stage4_rust_nested_module_resolves_to_sub_item_parent_file() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/foo")).unwrap();
    write_minimal_cargo_toml(repo.path(), "stage4_rust_nested");

    fs::write(
        repo.path().join("src/foo/mod.rs"),
        "// foo mod\npub mod bar;\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/foo/bar.rs"),
        "// bar\npub struct Thing;\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.rs"),
        "// main\npub mod foo;\nuse crate::foo::bar::Thing;\nfn main() { let _: Thing; }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.rs").unwrap().unwrap();
    let bar_file = graph.file_by_path("src/foo/bar.rs").unwrap().unwrap();
    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(bar_file.id)),
        "expected Imports edge from src/main.rs to src/foo/bar.rs; got: {imports:?}"
    );
}

/// `super::b::X` from `src/foo/a.rs` resolves to `src/foo/b.rs`.
#[test]
fn stage4_rust_super_prefix_walks_one_level_up() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/foo")).unwrap();
    write_minimal_cargo_toml(repo.path(), "stage4_rust_super");

    fs::write(
        repo.path().join("src/foo/a.rs"),
        "// a\nuse super::b::X;\npub fn consume(_: X) {}\n",
    )
    .unwrap();
    fs::write(repo.path().join("src/foo/b.rs"), "// b\npub struct X;\n").unwrap();
    fs::write(
        repo.path().join("src/foo/mod.rs"),
        "pub mod a;\npub mod b;\n",
    )
    .unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub mod foo;\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let a_file = graph.file_by_path("src/foo/a.rs").unwrap().unwrap();
    let b_file = graph.file_by_path("src/foo/b.rs").unwrap().unwrap();
    let imports = graph
        .outbound(NodeId::File(a_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(b_file.id)),
        "expected Imports edge from src/foo/a.rs to src/foo/b.rs; got: {imports:?}"
    );
}

/// External-crate `use std::collections::HashMap;` does not emit an Imports
/// edge because no candidate exists under the crate `src/`.
#[test]
fn stage4_rust_external_crate_use_is_skipped_silently() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    write_minimal_cargo_toml(repo.path(), "stage4_rust_std");

    fs::write(
        repo.path().join("src/lib.rs"),
        "use std::collections::HashMap;\npub fn noop() -> HashMap<u8, u8> { HashMap::new() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let imports = graph
        .outbound(NodeId::File(lib_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.is_empty(),
        "external-crate `use std::...` must not emit Imports edge; got: {imports:?}"
    );
}

// ── Go import resolution ─────────────────────────────────────────────────────

/// Go import of `<module>/a` fans out to every `.go` file in `a/`.
#[test]
fn stage4_go_import_fans_out_to_every_file_in_package() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("a")).unwrap();
    fs::create_dir_all(repo.path().join("b")).unwrap();
    fs::write(
        repo.path().join("go.mod"),
        "module example.com/stage4go\n\ngo 1.21\n",
    )
    .unwrap();

    fs::write(
        repo.path().join("a/a.go"),
        "package a\n\nfunc Hello() string { return \"hi\" }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("a/a_util.go"),
        "package a\n\nfunc Util() int { return 1 }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("b/b.go"),
        "package b\n\nimport \"example.com/stage4go/a\"\n\nfunc Use() string { return a.Hello() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    let b_file = graph
        .file_by_path("b/b.go")
        .unwrap()
        .unwrap_or_else(|| panic!("b/b.go missing; graph has: {paths:?}"));
    let a_file = graph
        .file_by_path("a/a.go")
        .unwrap()
        .unwrap_or_else(|| panic!("a/a.go missing; graph has: {paths:?}"));
    let a_util_file = graph
        .file_by_path("a/a_util.go")
        .unwrap()
        .unwrap_or_else(|| panic!("a/a_util.go missing; graph has: {paths:?}"));

    let imports = graph
        .outbound(NodeId::File(b_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(a_file.id)),
        "expected Imports edge from b/b.go to a/a.go; got: {imports:?}"
    );
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(a_util_file.id)),
        "expected Imports edge from b/b.go to a/a_util.go; got: {imports:?}"
    );
}

/// External Go imports (no `go.mod` prefix match) are skipped silently.
#[test]
fn stage4_go_external_import_is_skipped_silently() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("go.mod"),
        "module example.com/stage4ext\n\ngo 1.21\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("main.go"),
        "package main\n\nimport \"fmt\"\n\nfunc main() { fmt.Println(\"hi\") }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("main.go").unwrap().unwrap();
    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports.is_empty(),
        "external Go import `fmt` must not emit Imports edge; got: {imports:?}"
    );
}
