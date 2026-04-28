use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use super::common::{assert_symbol_call, symbol_named};
use crate::config::Config;
use std::fs;
use tempfile::tempdir;

#[test]
fn stage4_emits_symbol_calls_edge_for_rust_call() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/lib.rs"),
        "fn helper() {}\nfn entry() { helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let lib_file = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let entry = symbol_named(&graph, lib_file.id, "entry");
    let helper = symbol_named(&graph, lib_file.id, "helper");
    assert_symbol_call(&graph, entry, helper);
}

#[test]
fn stage4_emits_symbol_calls_edge_for_python_call() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("main.py"),
        "def helper():\n    pass\n\ndef entry():\n    helper()\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("main.py").unwrap().unwrap();
    let entry = symbol_named(&graph, main_file.id, "entry");
    let helper = symbol_named(&graph, main_file.id, "helper");
    assert_symbol_call(&graph, entry, helper);
}

#[test]
fn stage4_emits_symbol_calls_edge_for_typescript_call() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "export function helper() {}\nexport function entry() { helper(); }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let entry = symbol_named(&graph, main_file.id, "entry");
    let helper = symbol_named(&graph, main_file.id, "helper");
    assert_symbol_call(&graph, entry, helper);
}

#[test]
fn stage4_emits_symbol_calls_edge_for_go_call() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("main.go"),
        "package main\nfunc Helper() {}\nfunc main() { Helper() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("main.go").unwrap().unwrap();
    let caller = symbol_named(&graph, main_file.id, "main");
    let helper = symbol_named(&graph, main_file.id, "Helper");
    assert_symbol_call(&graph, caller, helper);
}
