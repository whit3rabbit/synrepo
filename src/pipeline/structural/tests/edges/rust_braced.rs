//! Stage-4 edge tests for Rust braced-use imports.
//!
//! `use foo::{a, b};` fans out into two leaf captures in the import
//! query (see `structure/parse/language.rs::RUST_IMPORT_QUERY`). Stage
//! 4 deduplicates via `HashSet<FileNodeId>` so a single `Imports` edge
//! is emitted from the importing file to the imported module file.

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
fn stage4_emits_imports_edge_for_rust_braced_use() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    write_minimal_cargo_toml(repo.path(), "stage4_rust_braced");

    fs::write(
        repo.path().join("src/util.rs"),
        "// util module\npub fn helper_a() {}\npub fn helper_b() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "// lib crate\npub mod util;\nuse crate::util::{helper_a, helper_b};\nfn entry() { helper_a(); helper_b(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let util_file = graph.file_by_path("src/util.rs").unwrap().unwrap();

    let imports = graph
        .outbound(NodeId::File(lib_file.id), Some(EdgeKind::Imports))
        .unwrap();
    let matching: Vec<_> = imports
        .iter()
        .filter(|e| e.to == NodeId::File(util_file.id))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "braced-use must emit exactly one Imports edge (deduped); got: {matching:?} in {imports:?}"
    );
}
