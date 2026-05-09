use std::fs;

use synrepo::{
    config::Config,
    core::{
        ids::NodeId,
        project_layout::{detect_project_layout, ProjectProfileKind},
    },
    pipeline::structural::run_structural_compile,
    store::sqlite::SqliteGraphStore,
    structure::graph::EdgeKind,
    surface::card::{compiler::GraphCardCompiler, Budget, CardCompiler, EntryPointKind},
};
use tempfile::tempdir;

#[test]
fn android_repo_discovers_jvm_sources_entrypoint_imports_and_tests() {
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("settings.gradle"), "include ':app'\n").unwrap();
    fs::create_dir_all(repo.path().join("app/src/main/java/com/example")).unwrap();
    fs::create_dir_all(repo.path().join("app/src/main/kotlin/com/example")).unwrap();
    fs::create_dir_all(repo.path().join("app/src/test/java/com/example")).unwrap();

    fs::write(
        repo.path().join("app/src/main/AndroidManifest.xml"),
        r#"<manifest package="com.example">
  <application>
    <activity android:name=".MainActivity">
      <intent-filter>
        <action android:name="android.intent.action.MAIN" />
        <category android:name="android.intent.category.LAUNCHER" />
      </intent-filter>
    </activity>
  </application>
</manifest>"#,
    )
    .unwrap();
    fs::write(
        repo.path()
            .join("app/src/main/java/com/example/MainActivity.java"),
        "package com.example;\nimport com.example.Shell;\npublic class MainActivity {}\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("app/src/main/kotlin/com/example/Shell.kt"),
        "package com.example\nclass Shell { fun run() {} }\n",
    )
    .unwrap();
    fs::write(
        repo.path()
            .join("app/src/test/java/com/example/ShellTest.java"),
        "package com.example;\npublic class ShellTest { public void usesShell() {} }\n",
    )
    .unwrap();

    let config = Config::default();
    let layout = detect_project_layout(repo.path(), &config.roots);
    assert!(layout
        .profiles
        .iter()
        .any(|profile| profile.kind == ProjectProfileKind::Android));
    assert!(layout
        .source_roots
        .contains(&"app/src/main/java".to_string()));
    assert!(layout
        .source_roots
        .contains(&"app/src/main/kotlin".to_string()));

    let graph_dir = repo.path().join(".synrepo/graph");
    let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let activity_file = graph
        .file_by_path("app/src/main/java/com/example/MainActivity.java")
        .unwrap()
        .unwrap();
    assert_eq!(activity_file.language.as_deref(), Some("java"));
    let shell_file = graph
        .file_by_path("app/src/main/kotlin/com/example/Shell.kt")
        .unwrap()
        .unwrap();
    assert_eq!(shell_file.language.as_deref(), Some("kotlin"));

    let imports = graph
        .outbound(NodeId::File(activity_file.id), Some(EdgeKind::Imports))
        .unwrap();
    assert!(
        imports
            .iter()
            .any(|edge| edge.to == NodeId::File(shell_file.id)),
        "expected Java import edge to Kotlin Shell; got: {imports:?}"
    );

    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let entrypoints = compiler.entry_point_card(None, Budget::Tiny).unwrap();
    assert!(
        entrypoints.entry_points.iter().any(|entry| {
            entry.kind == EntryPointKind::Binary
                && entry.qualified_name == "MainActivity"
                && entry
                    .location
                    .starts_with("app/src/main/java/com/example/MainActivity.java:")
        }),
        "expected Android launcher entrypoint, got: {:?}",
        entrypoints.entry_points
    );

    let tests = compiler
        .test_surface_card("app/src/main/kotlin/com/example/Shell.kt", Budget::Normal)
        .unwrap();
    assert!(
        tests
            .tests
            .iter()
            .any(|entry| entry.file_path == "app/src/test/java/com/example/ShellTest.java"),
        "expected ShellTest.java association, got: {:?}",
        tests.tests
    );
}
