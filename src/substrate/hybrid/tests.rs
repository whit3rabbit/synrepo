use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn hybrid_search_falls_back_to_lexical_without_semantic_assets() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo/index")).unwrap();
    fs::write(repo.path().join("README.md"), "alpha token\n").unwrap();
    let config = Config::default();
    crate::substrate::index::build_index(&config, repo.path()).unwrap();

    let report = hybrid_search(&config, repo.path(), "alpha", &SearchOptions::default()).unwrap();
    assert!(!report.semantic_available);
    assert_eq!(report.engine, "syntext");
    assert_eq!(report.rows[0].source, HybridSearchSource::Lexical);
}

#[cfg(feature = "semantic-triage")]
#[test]
fn hybrid_search_uses_existing_semantic_index_when_enabled() {
    use crate::config::SemanticEmbeddingProvider;
    use crate::core::ids::{FileNodeId, SymbolNodeId};
    use crate::substrate::embedding::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};

    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo/index/vectors")).unwrap();
    fs::write(repo.path().join("README.md"), "lexical only\n").unwrap();
    let config = Config {
        enable_semantic_triage: true,
        semantic_embedding_provider: SemanticEmbeddingProvider::Ollama,
        semantic_model: "all-minilm".to_string(),
        embedding_dim: 2,
        semantic_ollama_endpoint: spawn_embedding_server(2),
        ..Config::default()
    };
    crate::substrate::index::build_index(&config, repo.path()).unwrap();

    let model = crate::substrate::embedding::model::ModelResolver::new()
        .resolve(&config, &Config::synrepo_dir(repo.path()))
        .unwrap();
    let index = crate::substrate::embedding::FlatVecIndex::build(
        vec![EmbeddingChunk {
            id: ChunkId(1),
            source: EmbeddingChunkSource::Symbol {
                id: SymbolNodeId(1),
                file_id: FileNodeId(1),
                qualified_name: "semantic::target".to_string(),
                kind_label: "function".to_string(),
            },
            text: "semantic target".to_string(),
        }],
        model,
    )
    .unwrap();
    index
        .save(&Config::synrepo_dir(repo.path()).join("index/vectors/index.bin"))
        .unwrap();

    let report = hybrid_search(
        &config,
        repo.path(),
        "meaning based query",
        &SearchOptions::default(),
    )
    .unwrap();

    assert!(report.semantic_available);
    assert_eq!(report.engine, "syntext+vectors");
    assert!(report
        .rows
        .iter()
        .any(|row| row.source == HybridSearchSource::Semantic));
}

#[cfg(feature = "semantic-triage")]
#[test]
fn hybrid_search_falls_back_to_lexical_when_ollama_query_fails() {
    use crate::config::SemanticEmbeddingProvider;
    use crate::core::ids::{FileNodeId, SymbolNodeId};
    use crate::substrate::embedding::chunk::{ChunkId, EmbeddingChunk, EmbeddingChunkSource};

    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo/index/vectors")).unwrap();
    fs::write(repo.path().join("README.md"), "alpha token\n").unwrap();
    let mut config = Config {
        enable_semantic_triage: true,
        semantic_embedding_provider: SemanticEmbeddingProvider::Ollama,
        semantic_model: "all-minilm".to_string(),
        embedding_dim: 2,
        semantic_ollama_endpoint: spawn_one_embedding_server(),
        ..Config::default()
    };
    crate::substrate::index::build_index(&config, repo.path()).unwrap();

    let model = crate::substrate::embedding::model::ModelResolver::new()
        .resolve(&config, &Config::synrepo_dir(repo.path()))
        .unwrap();
    let index = crate::substrate::embedding::FlatVecIndex::build(
        vec![EmbeddingChunk {
            id: ChunkId(1),
            source: EmbeddingChunkSource::Symbol {
                id: SymbolNodeId(1),
                file_id: FileNodeId(1),
                qualified_name: "alpha::token".to_string(),
                kind_label: "function".to_string(),
            },
            text: "alpha token".to_string(),
        }],
        model,
    )
    .unwrap();
    index
        .save(&Config::synrepo_dir(repo.path()).join("index/vectors/index.bin"))
        .unwrap();

    config.semantic_ollama_endpoint = "http://127.0.0.1:9".to_string();
    let report = hybrid_search(&config, repo.path(), "alpha", &SearchOptions::default()).unwrap();
    assert!(!report.semantic_available);
    assert_eq!(report.engine, "syntext");
    assert_eq!(report.rows[0].source, HybridSearchSource::Lexical);
}

#[cfg(feature = "semantic-triage")]
fn spawn_one_embedding_server() -> String {
    spawn_embedding_server(1)
}

#[cfg(feature = "semantic-triage")]
fn spawn_embedding_server(requests: usize) -> String {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for _ in 0..requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer).unwrap();
            let body = r#"{"embeddings":[[1.0,0.0]]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    format!("http://{addr}")
}
