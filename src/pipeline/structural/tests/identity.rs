use super::support::open_graph;
use crate::config::Config;
use std::fs;
use tempfile::tempdir;

#[test]
fn git_rename_fallback_preserves_file_id_when_content_changes() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Initialize git repo.
    crate::pipeline::git::test_support::git(&repo, &["init"]);
    crate::pipeline::git::test_support::git(&repo, &["config", "user.name", "test"]);
    crate::pipeline::git::test_support::git(&repo, &["config", "user.email", "test@example.com"]);

    // Create a file large enough that git rename detection (similarity-based)
    // can identify it after a rename with minor content changes.
    let original_content = "\
/// Documentation for the original function.
/// This comment makes the file large enough for rename detection.
pub fn original_fn() -> i32 {
    let x = 1;
    let y = 2;
    x + y
}

/// Documentation for the helper function.
/// Another comment line for similarity matching.
pub fn helper_fn() -> i32 {
    42
}
";
    fs::write(repo.path().join("src/old_name.rs"), original_content).unwrap();
    crate::pipeline::git::test_support::git(&repo, &["add", "."]);
    crate::pipeline::git::test_support::git(&repo, &["commit", "-m", "initial"]);

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    let file_before = graph.file_by_path("src/old_name.rs").unwrap().unwrap();
    let file_id_before = file_before.id;

    // Rename and change one return type (high similarity, different content hash).
    let renamed_content = "\
/// Documentation for the original function.
/// This comment makes the file large enough for rename detection.
pub fn original_fn() -> i64 {
    let x = 1;
    let y = 2;
    x + y
}

/// Documentation for the helper function.
/// Another comment line for similarity matching.
pub fn helper_fn() -> i32 {
    42
}
";
    fs::remove_file(repo.path().join("src/old_name.rs")).unwrap();
    fs::write(repo.path().join("src/new_name.rs"), renamed_content).unwrap();
    crate::pipeline::git::test_support::git(&repo, &["add", "."]);
    crate::pipeline::git::test_support::git(&repo, &["commit", "-m", "rename and modify"]);

    let summary = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Verify new file is in graph and old is gone.
    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(
        paths.contains(&"src/new_name.rs".to_string()),
        "renamed file should be in graph, got: {paths:?}"
    );
    assert!(
        !paths.contains(&"src/old_name.rs".to_string()),
        "old path should be gone"
    );

    // The identity cascade should have resolved at least one rename.
    assert!(
        summary.identities_resolved > 0,
        "should have resolved at least one identity via git rename fallback"
    );

    // Verify FileNodeId was preserved.
    let file_after = graph.file_by_path("src/new_name.rs").unwrap().unwrap();
    assert_eq!(
        file_id_before, file_after.id,
        "FileNodeId should be preserved across git-detected rename"
    );
}

#[test]
fn file_split_produces_split_from_edges() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Create a file with multiple symbols.
    fs::write(
        repo.path().join("src/big.rs"),
        "pub fn alpha() -> i32 { 1 }\npub fn beta() -> i32 { 2 }\npub fn gamma() -> i32 { 3 }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Now split the file into two.
    fs::remove_file(repo.path().join("src/big.rs")).unwrap();
    fs::write(
        repo.path().join("src/alpha.rs"),
        "pub fn alpha() -> i32 { 1 }\npub fn beta() -> i32 { 2 }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/gamma.rs"),
        "pub fn gamma() -> i32 { 3 }\n",
    )
    .unwrap();

    let summary = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // The identity cascade should have detected the split.
    assert!(
        summary.identities_resolved > 0,
        "should have resolved at least one identity (split)"
    );

    // Verify both new files are in the graph.
    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(
        paths.contains(&"src/alpha.rs".to_string()),
        "alpha.rs should be in graph"
    );
    assert!(
        paths.contains(&"src/gamma.rs".to_string()),
        "gamma.rs should be in graph"
    );
    assert!(
        !paths.contains(&"src/big.rs".to_string()),
        "big.rs should be gone"
    );
}

#[test]
fn file_merge_produces_merged_from_edges() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Create two separate files.
    fs::write(
        repo.path().join("src/a.rs"),
        "pub fn func_a() -> i32 { 1 }\npub fn helper_a() -> i32 { 2 }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "pub fn func_b() -> i32 { 3 }\npub fn helper_b() -> i32 { 4 }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Now merge both into one file.
    fs::remove_file(repo.path().join("src/a.rs")).unwrap();
    fs::remove_file(repo.path().join("src/b.rs")).unwrap();
    fs::write(
        repo.path().join("src/combined.rs"),
        "pub fn func_a() -> i32 { 1 }\npub fn helper_a() -> i32 { 2 }\npub fn func_b() -> i32 { 3 }\npub fn helper_b() -> i32 { 4 }\n",
    )
    .unwrap();

    let summary = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // The identity cascade should have detected the merge.
    assert!(
        summary.identities_resolved > 0,
        "should have resolved at least one identity (merge)"
    );

    // Verify the combined file is in the graph and the old ones are gone.
    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(
        paths.contains(&"src/combined.rs".to_string()),
        "combined.rs should be in graph"
    );
    assert!(
        !paths.contains(&"src/a.rs".to_string()),
        "a.rs should be gone"
    );
    assert!(
        !paths.contains(&"src/b.rs".to_string()),
        "b.rs should be gone"
    );
}

#[test]
fn no_match_produces_breakage_not_crash() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Create a file and compile.
    fs::write(
        repo.path().join("src/unique.rs"),
        "pub fn unique_symbol_xyz() -> i32 { 42 }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Delete the file without creating a replacement with overlapping symbols.
    fs::remove_file(repo.path().join("src/unique.rs")).unwrap();
    fs::write(
        repo.path().join("src/unrelated.rs"),
        "pub fn completely_different() -> i32 { 99 }\n",
    )
    .unwrap();

    // Should not panic or error.
    let result = super::super::run_structural_compile(repo.path(), &config, &mut graph);
    assert!(result.is_ok(), "compile should succeed even with breakage");

    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(
        !paths.contains(&"src/unique.rs".to_string()),
        "deleted file should be gone"
    );
    assert!(
        paths.contains(&"src/unrelated.rs".to_string()),
        "new unrelated file should be in graph"
    );
}
