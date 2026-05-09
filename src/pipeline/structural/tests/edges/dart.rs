use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

#[test]
fn stage4_emits_imports_edge_for_dart_package_and_relative_imports() {
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("pubspec.yaml"),
        "name: ottershell\ndependencies:\n  flutter:\n    sdk: flutter\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join("lib/src")).unwrap();

    fs::write(repo.path().join("lib/src/shell.dart"), "class Shell {}\n").unwrap();
    fs::write(repo.path().join("lib/src/local.dart"), "class Local {}\n").unwrap();
    fs::write(
        repo.path().join("lib/main.dart"),
        "import 'dart:core';\n\
         import 'package:ottershell/src/shell.dart';\n\
         import 'src/local.dart';\n\
         void main() {}\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("lib/main.dart").unwrap().unwrap();
    let shell_file = graph.file_by_path("lib/src/shell.dart").unwrap().unwrap();
    let local_file = graph.file_by_path("lib/src/local.dart").unwrap().unwrap();

    let imports = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Imports))
        .unwrap();

    assert!(
        imports.iter().any(|e| e.to == NodeId::File(shell_file.id)),
        "expected Dart package import edge from lib/main.dart to lib/src/shell.dart; got: {imports:?}"
    );
    assert!(
        imports.iter().any(|e| e.to == NodeId::File(local_file.id)),
        "expected Dart relative import edge from lib/main.dart to lib/src/local.dart; got: {imports:?}"
    );
    assert_eq!(
        imports.len(),
        2,
        "dart: SDK imports must not emit graph imports; got: {imports:?}"
    );
}
