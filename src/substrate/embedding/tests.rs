//! End-to-end embedding test (Task 4).
//!
//! Walks the full pipeline a user-triggered semantic-triage run exercises:
//! tempdir repo → bootstrap → graph open → `extract_chunks` → load on-disk
//! v3 index → query. Asserts the chunk extractor sees both source variants
//! and that the loaded index returns at least one positive-scoring hit.

use crate::bootstrap::bootstrap;
use crate::config::Config;
use crate::store::sqlite::SqliteGraphStore;
use crate::substrate::embedding::chunk::extract_chunks;
use crate::substrate::embedding::{load_embedding_index, EmbeddingChunkSource};
use tempfile::tempdir;

#[test]
fn bootstrap_extract_chunks_load_v3_index_and_query() {
    let repo = tempdir().expect("tempdir");
    let repo_path = repo.path();

    // Two byte-unique Rust files. `FileNodeId` is content-hashed for new files
    // (CLAUDE.md: "Test fixtures that create multiple files must not share
    // byte-identical content"), so the bodies must differ.
    std::fs::create_dir_all(repo_path.join("src")).unwrap();
    std::fs::write(
        repo_path.join("src/lib.rs"),
        "pub fn parse_config() -> u32 { 0 }\n",
    )
    .unwrap();
    std::fs::write(
        repo_path.join("src/util.rs"),
        "// util helpers\npub fn render_diagram() -> &'static str { \"\" }\n",
    )
    .unwrap();

    // One concept file under the default concept directory.
    std::fs::create_dir_all(repo_path.join("docs/concepts")).unwrap();
    std::fs::write(
        repo_path.join("docs/concepts/parser.md"),
        "# Parser\n\nDocs about parsing configuration files.\n",
    )
    .unwrap();

    // Pre-write the config so bootstrap turns on semantic triage. This both
    // exercises the disk-side index build inside bootstrap and gives
    // `load_embedding_index` something to load below.
    let synrepo_dir = Config::synrepo_dir(repo_path);
    std::fs::create_dir_all(&synrepo_dir).unwrap();
    std::fs::write(
        synrepo_dir.join("config.toml"),
        "enable_semantic_triage = true\n",
    )
    .unwrap();

    bootstrap(repo_path, None, false).expect("bootstrap");

    // Open the populated graph and pull chunks via the public extractor.
    let graph_dir = synrepo_dir.join("graph");
    let store = SqliteGraphStore::open(&graph_dir).expect("open graph store");
    let chunks = extract_chunks(&store).expect("extract_chunks");

    // Two functions + one concept => at least three chunks. Exact count is
    // brittle (depends on parser surfacing nothing else), so use a lower bound.
    assert!(
        chunks.len() >= 3,
        "expected at least 3 chunks, got {}",
        chunks.len()
    );
    assert!(
        chunks
            .iter()
            .any(|c| matches!(c.source, EmbeddingChunkSource::Symbol { .. })),
        "expected at least one Symbol-sourced chunk"
    );
    assert!(
        chunks
            .iter()
            .any(|c| matches!(c.source, EmbeddingChunkSource::Concept { .. })),
        "expected at least one Concept-sourced chunk"
    );

    // Reload the on-disk index. This exercises the v3 read path (16-byte
    // chunk id field) end-to-end: a regression to the old 8-byte format
    // would surface here as a deserialization error or a wrong-id round trip.
    let config = Config::load(repo_path).expect("config load");
    let index = load_embedding_index(&config, &synrepo_dir)
        .expect("load_embedding_index")
        .expect("semantic triage enabled, index should be Some");
    assert!(!index.is_empty(), "loaded index must not be empty");

    // Query against text that lexically overlaps one of the symbols. We
    // assert only that a hit comes back with a positive score; we
    // deliberately do not pin which chunk ranks first to keep this stable
    // across model minor versions.
    let query_vec = index
        .embed_text("parse configuration")
        .expect("embed_text query");
    let results = index.query(&query_vec, 5);
    assert!(!results.is_empty(), "expected at least one query hit");
    assert!(
        results[0].1 > 0.0,
        "top hit score should be positive, got {}",
        results[0].1
    );
}
