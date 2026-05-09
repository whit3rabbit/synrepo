use std::fs;

use synrepo::{
    config::Config,
    core::ids::NodeId,
    pipeline::structural::run_structural_compile,
    store::sqlite::SqliteGraphStore,
    structure::graph::EdgeKind,
    surface::card::{compiler::GraphCardCompiler, Budget, CardCompiler, EntryPointKind},
};
use tempfile::tempdir;

#[test]
fn flutter_repo_discovers_dart_sources_entrypoint_and_tests() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("pubspec.yaml"),
        "name: ottershell\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join("lib/src")).unwrap();
    fs::create_dir_all(repo.path().join("test/src")).unwrap();

    fs::write(
        repo.path().join("lib/main.dart"),
        "import 'package:ottershell/src/shell.dart';\nvoid main() { Shell().run(); }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("lib/src/shell.dart"),
        "class Shell { void run() {} }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("test/src/shell_test.dart"),
        "// distinct test fixture\nimport 'package:ottershell/src/shell.dart';\nvoid main() { Shell().run(); }\n",
    )
    .unwrap();

    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    let summary = run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();

    let all_paths = graph.all_file_paths().unwrap();
    let main_file = graph
        .file_by_path("lib/main.dart")
        .unwrap()
        .unwrap_or_else(|| {
            panic!("missing lib/main.dart after {summary:?}; graph paths: {all_paths:?}")
        });
    assert_eq!(main_file.language.as_deref(), Some("dart"));
    let shell_file = graph.file_by_path("lib/src/shell.dart").unwrap().unwrap();
    let shell_symbols = graph.symbols_for_file(shell_file.id).unwrap();
    assert!(
        shell_symbols
            .iter()
            .any(|symbol| symbol.qualified_name == "Shell"),
        "expected Shell symbol, got: {shell_symbols:?}"
    );

    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports
            .iter()
            .any(|edge| edge.to == NodeId::File(shell_file.id)),
        "expected package import edge from lib/main.dart to lib/src/shell.dart; got: {imports:?}"
    );

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let entrypoints = compiler.entry_point_card(None, Budget::Tiny).unwrap();
    assert!(
        entrypoints.entry_points.iter().any(|entry| {
            entry.kind == EntryPointKind::Binary && entry.location.starts_with("lib/main.dart:")
        }),
        "expected Flutter lib/main.dart entrypoint, got: {:?}",
        entrypoints.entry_points
    );

    let tests = compiler
        .test_surface_card("lib/src/shell.dart", Budget::Normal)
        .unwrap();
    assert!(
        tests
            .tests
            .iter()
            .any(|entry| entry.file_path == "test/src/shell_test.dart"),
        "expected shell_test.dart association, got: {:?}",
        tests.tests
    );
}
