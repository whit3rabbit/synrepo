use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

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

/// TypeScript: exported fn callable via import.
#[test]
fn ts_export_fn_callable_via_import() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/util.ts"),
        "export function handle(x: number): number { return x; }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { handle } from './util'\nhandle(1)\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    assert!(
        !calls.is_empty(),
        "expected Calls edge to exported function"
    );
}

/// TypeScript: non-exported fn callable via import (parser defaults to Public).
#[test]
fn ts_non_exported_fn_callable_via_import() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Note: TS parser defaults non-exported to Public, so this DOES resolve.
    fs::write(
        repo.path().join("src/util.ts"),
        "function internal(x: number): number { return x; }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/main.ts"),
        "import { internal } from './util'\ninternal(1)\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let main_file = graph.file_by_path("src/main.ts").unwrap().unwrap();
    let calls = graph
        .outbound(NodeId::File(main_file.id), Some(EdgeKind::Calls))
        .unwrap();
    // TS parser defaults to Public, so this resolves.
    assert!(!calls.is_empty(), "expected Calls edge to function");
}
