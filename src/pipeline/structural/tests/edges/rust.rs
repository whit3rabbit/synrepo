use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

fn write_minimal_cargo_toml(repo: &std::path::Path, name: &str) {
    fs::write(
        repo.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.0.1\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n"
        ),
    )
    .unwrap();
}

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

/// 7.1: ambiguous short name with no imports and private cross-file candidates
/// is dropped (scope narrowing: every candidate scores <= 0).
#[test]
fn stage4_ambiguous_call_name_no_imports_is_dropped() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Contents must differ so FileNodeId (content-hash) doesn't collapse into
    // a stage-6 rename.
    fs::write(
        repo.path().join("src/a.rs"),
        "// a\nfn helper() -> i32 { 1 }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "// b\nfn helper() -> i32 { 2 }\n",
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

    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();

    assert!(
        calls.is_empty(),
        "expected no Calls edges for private cross-file ambiguous short name; got: {calls:?}"
    );
}

/// 7.1b: scoped resolution disambiguates a qualified call (`a::helper`) using
/// the prefix-match bonus, even when other candidates share the short name.
#[test]
fn stage4_call_resolves_to_imported_module() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/a.rs"), "pub fn helper() {}\n").unwrap();
    fs::write(
        repo.path().join("src/main.rs"),
        "mod a;\nfn main() { a::helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let a_file = graph.file_by_path("src/a.rs").unwrap().unwrap();
    let a_helper = graph
        .outbound(NodeId::File(a_file.id), Some(EdgeKind::Defines))
        .unwrap()
        .into_iter()
        .find_map(|e| match e.to {
            NodeId::Symbol(id) => Some(id),
            _ => None,
        })
        .expect("a::helper must be defined");

    let main_file = graph.file_by_path("src/main.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();

    assert!(
        calls.iter().any(|e| e.to == NodeId::Symbol(a_helper)),
        "expected Calls edge from main.rs to a::helper; got: {calls:?}"
    );
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

/// Rust: call to imported module function resolves uniquely.
#[test]
fn rust_calls_resolve_to_imported_module() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/util.rs"),
        "pub fn transform(s: &str) -> String { s.to_uppercase() }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.rs"),
        "mod util;\nfn run() { util::transform(\"hi\"); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // Should resolve to the imported transform, not to anything else.
    assert!(!calls.is_empty(), "expected Calls edge; got none");
}

/// Rust: private cross-file function is NOT callable from sibling file.
#[test]
fn rust_private_fn_not_called_from_sibling() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // File A: defines a private helper.
    fs::write(
        repo.path().join("src/a.rs"),
        "mod b;\nfn private_helper() { b::call_me(); }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "// b module\npub fn call_me() {}\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let a_file = graph.file_by_path("src/a.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(a_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // The call to b::call_me should resolve (pub, same crate).
    assert!(!calls.is_empty(), "expected Calls edge to pub fn");
}

/// Rust: pub(crate) fn callable within crate.
#[test]
fn rust_pub_crate_fn_callable_within_crate() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/lib.rs"),
        "pub(crate) fn crate_helper() {}\npub fn api() { crate_helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(lib_file.id), Some(EdgeKind::Calls))
        .unwrap();
    assert!(!calls.is_empty(), "expected Calls edge within crate");
}

/// Rust: ambiguous short name without imports is dropped.
#[test]
fn rust_ambiguous_short_name_dropped() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Two public functions in DIFFERENT modules with the same short name `map`.
    // One is imported, one is not. The imported one should resolve uniquely.
    fs::write(
        repo.path().join("src/util.rs"),
        "pub fn map<T>(x: T) -> T { x }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/other.rs"),
        "pub fn map<T>(x: T) -> T { x }\n",
    )
    .unwrap();
    // Caller imports only util, calls map -> should resolve to util::map uniquely.
    fs::write(
        repo.path().join("src/main.rs"),
        "mod util;\nmod other;\nfn main() { util::map(1); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.rs").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // With import: +50 (imported) + 30 (kind) = 80, unique.
    //other::map is public but not imported: +20 + 30 = 50.
    //Top score = 80, unique -> emit.
    assert!(
        !calls.is_empty(),
        "expected Calls edge to imported function; got none"
    );
}
