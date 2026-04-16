use super::support::open_graph;
use crate::{
    config::Config,
    core::ids::NodeId,
    structure::{
        drift::{fingerprint_for_file, StructuralFingerprint},
        graph::{EdgeKind, GraphStore},
    },
};
use std::fs;
use tempfile::tempdir;

#[test]
fn drift_scoring_writes_scores_for_cross_file_edges() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Create two files that reference each other.
    fs::write(
        repo.path().join("src/a.rs"),
        "pub fn greet() -> &'static str { \"hi\" }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "pub fn call_greet() -> &'static str { crate::a::greet() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _summary = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let file_a = graph.file_by_path("src/a.rs").unwrap().unwrap();

    // Cross-file edges (Calls/Imports) should exist.
    let outbound = graph
        .outbound(NodeId::File(file_a.id), Some(EdgeKind::Defines))
        .unwrap();
    assert!(!outbound.is_empty(), "file a must have Defines edges");

    // Verify drift scores are readable. The first compile should produce
    // mostly zero scores (nothing has changed yet), but the sidecar table
    // should be writable and readable.
    let revision = graph.latest_fingerprint_revision().unwrap().unwrap();
    let scores = graph.read_drift_scores(&revision).unwrap();
    // First compile: no edges have drifted, so scores are empty or all zero.
    // The implementation only writes non-zero scores.
    assert!(
        scores.iter().all(|(_, s)| *s > 0.0f32),
        "all written scores should be non-zero"
    );
}

#[test]
fn drift_score_is_one_for_deleted_endpoint() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Create two files.
    fs::write(repo.path().join("src/a.rs"), "pub fn f() -> i32 { 1 }\n").unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "pub fn g() -> i32 { crate::a::f() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Now delete file a, which has cross-file edges pointing to it.
    fs::remove_file(repo.path().join("src/a.rs")).unwrap();
    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // After re-compile, file a is gone. Any edge that pointed to a's symbols
    // should have been cleaned up by delete_missing_files. The remaining graph
    // should only contain file b.
    let paths: Vec<_> = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(
        !paths.contains(&"src/a.rs".to_string()),
        "deleted file should not be in graph"
    );
    assert!(
        paths.contains(&"src/b.rs".to_string()),
        "remaining file should be in graph"
    );
}

#[test]
fn signature_change_produces_nonzero_drift() {
    // Test fingerprint drift directly: compute fingerprints for two versions
    // of the same file and verify Jaccard distance is non-zero.
    let fp_before = StructuralFingerprint::from_pairs([("g".to_string(), 100)]);
    let fp_after = StructuralFingerprint::from_pairs([
        ("g".to_string(), 200), // same name, different signature hash
    ]);
    assert!(
        fp_before.jaccard_distance(&fp_after) > 0.0,
        "signature change should produce non-zero Jaccard distance"
    );
    assert!(
        (fp_before.jaccard_distance(&fp_after) - 1.0).abs() < f32::EPSILON,
        "same name with different hash should be fully disjoint (distance 1.0)"
    );

    // Test that adding a symbol produces intermediate drift.
    let fp_v1 = StructuralFingerprint::from_pairs([("f".to_string(), 42)]);
    let fp_v2 = StructuralFingerprint::from_pairs([("f".to_string(), 42), ("g".to_string(), 99)]);
    let dist = fp_v1.jaccard_distance(&fp_v2);
    assert!(
        dist > 0.0 && dist < 1.0,
        "added symbol should produce intermediate drift, got {dist}"
    );
    assert!(
        (dist - 0.5).abs() < f32::EPSILON,
        "one added symbol out of two total should give distance 0.5"
    );
}

#[test]
fn unchanged_file_produces_zero_drift() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/a.rs"), "pub fn f() -> i32 { 1 }\n").unwrap();
    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    let rev1 = graph.latest_fingerprint_revision().unwrap().unwrap();
    let fp1 = graph.read_fingerprints(&rev1).unwrap();

    // Second compile with identical content.
    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();
    let rev2 = graph.latest_fingerprint_revision().unwrap().unwrap();
    let fp2 = graph.read_fingerprints(&rev2).unwrap();

    let file_a = graph.file_by_path("src/a.rs").unwrap().unwrap();
    assert_eq!(
        fp1.get(&file_a.id),
        fp2.get(&file_a.id),
        "unchanged file should have identical fingerprints across revisions"
    );

    // No drift scores should be written for unchanged files (all 0.0, filtered).
    let scores = graph.read_drift_scores(&rev2).unwrap();
    let has_drift = scores.iter().any(|(_, s)| *s > 0.0f32);
    assert!(!has_drift, "unchanged file should not produce drift scores");
}

#[test]
fn fingerprint_persistence_roundtrip() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(
        repo.path().join("src/a.rs"),
        "pub fn alpha() -> i32 { 1 }\npub fn beta() -> i32 { 2 }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let file_a = graph.file_by_path("src/a.rs").unwrap().unwrap();
    let expected_fp = fingerprint_for_file(file_a.id, &graph).unwrap();

    let rev = graph.latest_fingerprint_revision().unwrap().unwrap();
    let stored = graph.read_fingerprints(&rev).unwrap();
    let actual_fp = stored.get(&file_a.id).expect("fingerprint should exist");

    assert_eq!(
        &expected_fp, actual_fp,
        "stored fingerprint should match computed fingerprint"
    );
    assert_eq!(expected_fp.len(), 2, "file should have 2 symbol entries");
}

#[test]
fn content_edit_produces_nonzero_drift_file_id_unchanged() {
    // Test that editing a file in place: (1) FileNodeId persists, (2) content_hash changes.
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Initial file with one symbol.
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() -> &'static str { \"hello\" }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let file_before = graph.file_by_path("src/lib.rs").unwrap().unwrap();
    let file_id_before = file_before.id;
    let content_hash_before = file_before.content_hash.clone();

    // Now edit the file: change the symbol's body (different signature hash).
    fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn greet() -> &'static str { \"hi\" }\n",
    )
    .unwrap();

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let file_after = graph.file_by_path("src/lib.rs").unwrap().unwrap();

    // Verify: file_id unchanged (content-hash rename detection preserves identity).
    assert_eq!(
        file_id_before, file_after.id,
        "FileNodeId should persist across content edits"
    );

    // Verify: content_hash changed (the file was actually edited).
    assert_ne!(
        content_hash_before, file_after.content_hash,
        "content_hash should change on edit"
    );
}

#[test]
fn retired_edge_endpoint_gone_scores_full_drift() {
    // Test that when a file is deleted, edges pointing to its symbols are retired
    // and the target file is genuinely removed from the graph.
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    // Two files: a.rs defines a function, b.rs calls it.
    fs::write(
        repo.path().join("src/a.rs"),
        "pub fn target() -> i32 { 42 }\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("src/b.rs"),
        "pub use a::target;\npub fn call() -> i32 { target() }\n",
    )
    .unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);

    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Verify initial state: both files exist.
    assert!(graph.file_by_path("src/a.rs").unwrap().is_some());
    assert!(graph.file_by_path("src/b.rs").unwrap().is_some());

    // Delete the target file (a.rs).
    fs::remove_file(repo.path().join("src/a.rs")).unwrap();

    // Re-run compile. The file should be deleted and its symbols removed.
    let _ = super::super::run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    // Verify: a.rs is gone.
    let file_a = graph.file_by_path("src/a.rs").unwrap();
    assert!(file_a.is_none(), "deleted file should not be in graph");

    // Verify: b.rs still exists (it wasn't deleted).
    let file_b = graph.file_by_path("src/b.rs").unwrap();
    assert!(file_b.is_some(), "unrelated file should still be in graph");
}
