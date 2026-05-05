use super::*;
use crate::core::ids::FileNodeId;
use crate::core::provenance::Provenance;
use crate::structure::graph::Epistemic;

#[test]
fn jaccard_similarity_identical_sets() {
    let a: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
    let b: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
    assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn jaccard_similarity_disjoint_sets() {
    let a: HashSet<String> = ["a".to_string()].into_iter().collect();
    let b: HashSet<String> = ["b".to_string()].into_iter().collect();
    assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
}

#[test]
fn jaccard_similarity_partial_overlap() {
    let a: HashSet<String> = ["a".to_string(), "b".to_string(), "c".to_string()]
        .into_iter()
        .collect();
    let b: HashSet<String> = ["a".to_string(), "b".to_string(), "d".to_string()]
        .into_iter()
        .collect();
    assert!((jaccard_similarity(&a, &b) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn jaccard_similarity_empty_both() {
    let a: HashSet<String> = HashSet::new();
    let b: HashSet<String> = HashSet::new();
    assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn jaccard_similarity_one_empty() {
    let a: HashSet<String> = ["a".to_string()].into_iter().collect();
    let b: HashSet<String> = HashSet::new();
    assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
}

fn file(id: u128, root: &str, path: &str, content: &[u8], size: u64) -> FileNode {
    FileNode {
        id: FileNodeId(id),
        root_id: root.to_string(),
        path: path.to_string(),
        path_history: Vec::new(),
        content_hash: hex::encode(blake3::hash(content).as_bytes()),
        content_sample_hashes: sampled_content_hashes(content),
        size_bytes: size,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: Provenance::structural("test", "rev", Vec::new()),
    }
}

fn symbols(id: FileNodeId, names: &[&str]) -> (FileNodeId, HashSet<String>) {
    (id, names.iter().map(|name| (*name).to_string()).collect())
}

#[test]
fn detect_rename_accepts_single_high_symbol_overlap() {
    let old = file(1, "primary", "src/old.rs", b"old", 3);
    let new = file(2, "primary", "src/new.rs", b"new", 3);
    let old_sets = HashMap::from([symbols(old.id, &["a", "b", "c", "d"])]);
    let new_sets = HashMap::from([symbols(new.id, &["a", "b", "c", "d", "e"])]);

    let got = detect_rename(&old, &[new], &old_sets, &new_sets).unwrap();
    assert!(matches!(
        got,
        Some(IdentityResolution::Rename { preserved_id, new_path })
            if preserved_id == old.id && new_path == "src/new.rs"
    ));
}

#[test]
fn detect_rename_rejects_cross_root_match() {
    let old = file(1, "primary", "src/old.rs", b"old", 3);
    let new = file(2, "worktree", "src/new.rs", b"new", 3);
    let old_sets = HashMap::from([symbols(old.id, &["a", "b", "c", "d"])]);
    let new_sets = HashMap::from([symbols(new.id, &["a", "b", "c", "d"])]);

    let got = detect_rename(&old, &[new], &old_sets, &new_sets).unwrap();
    assert!(got.is_none());
}

#[test]
fn detect_rename_uses_sampled_similarity_for_symbol_poor_files() {
    let old_content = (0..=255u8).collect::<Vec<_>>();
    let mut new_content = old_content.clone();
    new_content[4] = b'X';
    let old = file(
        1,
        "primary",
        "src/old.rs",
        &old_content,
        old_content.len() as u64,
    );
    let new = file(
        2,
        "primary",
        "src/new.rs",
        &new_content,
        new_content.len() as u64,
    );
    let old_sets = HashMap::from([symbols(old.id, &[])]);
    let new_sets = HashMap::from([symbols(new.id, &[])]);

    let got = detect_rename(&old, &[new], &old_sets, &new_sets).unwrap();
    assert!(matches!(got, Some(IdentityResolution::Rename { .. })));
}

#[test]
fn detect_rename_skips_large_symbol_poor_sample_similarity() {
    let content = vec![b'a'; (SAMPLE_FILE_SIZE_CAP_BYTES as usize) + 1];
    let mut old = file(1, "primary", "src/old.rs", &content, content.len() as u64);
    let mut new = file(2, "primary", "src/new.rs", &content, content.len() as u64);
    old.content_sample_hashes = vec![1, 2, 3];
    new.content_sample_hashes = vec![1, 2, 3];
    let old_sets = HashMap::from([symbols(old.id, &[])]);
    let new_sets = HashMap::from([symbols(new.id, &[])]);

    let got = detect_rename(&old, &[new], &old_sets, &new_sets).unwrap();
    assert!(got.is_none());
}
