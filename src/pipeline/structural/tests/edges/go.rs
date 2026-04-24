use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

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

/// Go: capitalized fn callable across packages.
#[test]
fn go_capitalized_fn_callable_across_packages() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/util")).unwrap();

    fs::write(
        repo.path().join("go.mod"),
        "module example.com/mysvc\n\ngo 1.21\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/util/util.go"),
        "package util\nfunc Handle() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.go"),
        "package main\nimport \"example.com/mysvc/util\"\nfunc main() { util.Handle() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.go").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    assert!(
        !calls.is_empty(),
        "expected Calls edge to capitalized function"
    );
}

/// Go: lowercase fn NOT callable from other package.
#[test]
fn go_lowercase_fn_not_callable_cross_package() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/internal")).unwrap();

    fs::write(
        repo.path().join("go.mod"),
        "module example.com/mysvc\n\ngo 1.21\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/internal/internal.go"),
        "package internal\nfunc helper() {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.go"),
        "package main\nimport \"example.com/mysvc/internal\"\nfunc main() { internal.helper() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.go").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // Lowercase (private) should not resolve.
    assert!(
        calls.is_empty(),
        "expected no Calls edge to lowercase cross-package function; got: {calls:?}"
    );
}
