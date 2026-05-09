use super::super::super::run_structural_compile;
use super::super::support::open_graph;
use crate::{config::Config, core::ids::NodeId, structure::graph::EdgeKind};
use std::fs;
use tempfile::tempdir;

#[test]
fn stage4_emits_imports_edge_for_android_java_to_kotlin_import() {
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("settings.gradle"), "include ':app'\n").unwrap();
    fs::create_dir_all(repo.path().join("app/src/main/java/com/example")).unwrap();
    fs::create_dir_all(repo.path().join("app/src/main/kotlin/com/example")).unwrap();
    fs::write(
        repo.path().join("app/src/main/AndroidManifest.xml"),
        r#"<manifest package="com.example"><application /></manifest>"#,
    )
    .unwrap();
    fs::write(
        repo.path().join("app/src/main/kotlin/com/example/Shell.kt"),
        "package com.example\nclass Shell { fun run() {} }\n",
    )
    .unwrap();
    fs::write(
        repo.path()
            .join("app/src/main/java/com/example/MainActivity.java"),
        "package com.example;\nimport com.example.Shell;\npublic class MainActivity {}\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let activity_file = graph
        .file_by_path("app/src/main/java/com/example/MainActivity.java")
        .unwrap()
        .unwrap();
    let shell_file = graph
        .file_by_path("app/src/main/kotlin/com/example/Shell.kt")
        .unwrap()
        .unwrap();
    let imports = graph
        .outbound(NodeId::File(activity_file.id), Some(EdgeKind::Imports))
        .unwrap();

    assert!(
        imports.iter().any(|e| e.to == NodeId::File(shell_file.id)),
        "expected Java import edge from MainActivity.java to Shell.kt; got: {imports:?}"
    );
}
