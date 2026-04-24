use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

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

/// Python: method call on imported class resolves to imported method.
#[test]
fn python_method_call_on_imported_class() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/util.py"),
        "class User:\n    def greet(self): pass\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.py"),
        "from util import User\nu = User()\nu.greet()\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.py").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // Should resolve to User.greet from util.
    assert!(!calls.is_empty(), "expected Calls edge");
}

/// Python: underscore-prefixed private is NOT callable from outside.
#[test]
fn python_underscore_private_not_callable_from_outside() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/a.py"), "def _helper(): pass\n").unwrap();
    fs::write(repo.path().join("src/b.py"), "import a\na._helper()\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let b_file = graph.file_by_path("src/b.py").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(b_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // Private cross-file should not resolve.
    assert!(
        calls.is_empty(),
        "expected no Calls edge to private function; got: {calls:?}"
    );
}
