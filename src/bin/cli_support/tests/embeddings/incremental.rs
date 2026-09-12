#[cfg(feature = "semantic-triage")]
use tempfile::tempdir;

#[cfg(feature = "semantic-triage")]
use synrepo::config::Config;

#[cfg(feature = "semantic-triage")]
use super::{bootstrap, enable_ollama_embeddings, spawn_recording_server};

#[test]
#[cfg(feature = "semantic-triage")]
fn incremental_refresh_noop_performs_zero_embed_calls_and_leaves_index_untouched() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn a() {}\npub fn b() {}\n",
    )
    .unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let server = spawn_recording_server();
    enable_ollama_embeddings(repo.path(), &server.endpoint);

    super::super::super::commands::embeddings_build_output(repo.path(), true).unwrap();
    let config = Config::load(repo.path()).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let index_path =
        synrepo::substrate::embedding::profile_index_path_for_config(&synrepo_dir, &config);
    let meta_before = std::fs::metadata(&index_path).unwrap();
    let mtime_before = meta_before.modified().unwrap();
    let bytes_before = std::fs::read(&index_path).unwrap();

    server
        .call_count
        .store(0, std::sync::atomic::Ordering::SeqCst);
    server.bodies.lock().unwrap().clear();

    let graph = synrepo::store::sqlite::SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap();
    let res = synrepo::substrate::embedding::refresh_existing_embedding_index(
        &graph,
        &config,
        &synrepo_dir,
    )
    .unwrap();
    assert!(res.is_some());
    assert_eq!(
        server.call_count.load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    let meta_after = std::fs::metadata(&index_path).unwrap();
    assert_eq!(meta_after.modified().unwrap(), mtime_before);
    let bytes_after = std::fs::read(&index_path).unwrap();
    assert_eq!(bytes_before, bytes_after);
}

#[test]
#[cfg(feature = "semantic-triage")]
fn incremental_refresh_one_changed_chunk_embeds_only_that_chunk() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let server = spawn_recording_server();
    enable_ollama_embeddings(repo.path(), &server.endpoint);

    super::super::super::commands::embeddings_build_output(repo.path(), true).unwrap();

    std::fs::write(
        repo.path().join("src/lib.rs"),
        "/// Computes the answer\npub fn alpha() -> u32 { 42 }\npub fn beta() {}\n",
    )
    .unwrap();
    super::super::super::commands::reconcile(repo.path(), false).unwrap();

    server
        .call_count
        .store(0, std::sync::atomic::Ordering::SeqCst);
    server.bodies.lock().unwrap().clear();

    let config = Config::load(repo.path()).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let graph = synrepo::store::sqlite::SqliteGraphStore::open(&synrepo_dir.join("graph")).unwrap();
    let summary = synrepo::substrate::embedding::refresh_existing_embedding_index(
        &graph,
        &config,
        &synrepo_dir,
    )
    .unwrap()
    .unwrap();

    assert_eq!(summary.chunks, 2);
    let recorded = server.bodies.lock().unwrap().clone();
    let doc_bodies: Vec<String> = recorded
        .into_iter()
        .filter(|b| !b.contains("preflight"))
        .collect();
    assert_eq!(doc_bodies.len(), 1);
    assert!(doc_bodies[0].contains("Computes the answer"));
    assert!(!doc_bodies[0].contains("beta"));
}
