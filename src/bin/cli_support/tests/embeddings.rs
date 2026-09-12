use tempfile::tempdir;

#[cfg(feature = "semantic-triage")]
use synrepo::config::Config;

use super::support::bootstrap_isolated as bootstrap;

#[test]
#[cfg(not(feature = "semantic-triage"))]
fn build_requires_semantic_feature() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "embeddings feature test\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();

    let err = super::super::commands::embeddings_build_output(repo.path(), false).unwrap_err();
    assert!(
        err.to_string().contains("not built with `semantic-triage`"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn clean_removes_stale_profiles_and_legacy_flat_index() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();

    let config = synrepo::config::Config::load(repo.path()).unwrap();
    let synrepo_dir = synrepo::config::Config::synrepo_dir(repo.path());
    let vectors_root = synrepo_dir.join("index/vectors");
    let active =
        synrepo::substrate::embedding::profile_index_path_for_config(&synrepo_dir, &config);
    std::fs::create_dir_all(active.parent().unwrap()).unwrap();
    std::fs::write(&active, b"active").unwrap();

    let stale_dir = vectors_root.join("deadbeefdeadbeef-onnx-old-d384-float32");
    std::fs::create_dir_all(&stale_dir).unwrap();
    std::fs::write(stale_dir.join("index.bin"), b"stale").unwrap();
    std::fs::write(vectors_root.join("index.bin"), b"legacy v5").unwrap();

    // Dry run reports both artifacts but removes nothing.
    let output = super::super::commands::embeddings_clean_output(repo.path(), false, true).unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["status"], "completed");
    assert_eq!(value["applied"], false);
    assert_eq!(value["candidates"].as_array().unwrap().len(), 2);
    assert!(active.exists());
    assert!(stale_dir.exists());
    assert!(vectors_root.join("index.bin").exists());

    // Apply removes the stale profile dir and the legacy flat v5 index while
    // keeping the active profile.
    let output = super::super::commands::embeddings_clean_output(repo.path(), true, true).unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["status"], "completed");
    assert_eq!(value["removed"].as_array().map(Vec::len), Some(2));
    assert!(active.exists());
    assert!(!stale_dir.exists());
    assert!(!vectors_root.join("index.bin").exists());
}

#[test]
#[cfg(feature = "semantic-triage")]
fn build_requires_enabled_config() {
    let repo = tempdir().unwrap();
    std::fs::write(repo.path().join("README.md"), "embeddings disabled test\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();

    let err = super::super::commands::embeddings_build_output(repo.path(), false).unwrap_err();
    assert!(
        err.to_string().contains("embeddings are disabled"),
        "unexpected error: {err:#}"
    );
}

#[test]
#[cfg(feature = "semantic-triage")]
fn build_reports_ollama_preflight_failure() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    enable_ollama_embeddings(repo.path(), "http://127.0.0.1:9");

    let err = super::super::commands::embeddings_build_output(repo.path(), false).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("preflight") || message.contains("Ollama embed request failed"),
        "unexpected error: {err:#}"
    );
}

#[test]
#[cfg(feature = "semantic-triage")]
fn build_writes_embedding_index_with_ollama() {
    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let endpoint = spawn_embedding_server();
    enable_ollama_embeddings(repo.path(), &endpoint);

    let output = super::super::commands::embeddings_build_output(repo.path(), true).unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["status"], "completed");
    assert_eq!(value["summary"]["chunks"], 1);
    let config = Config::load(repo.path()).unwrap();
    let expected = synrepo::substrate::embedding::profile_index_path_for_config(
        &Config::synrepo_dir(repo.path()),
        &config,
    );
    assert!(
        expected.exists(),
        "embedding index should be written at {}",
        expected.display()
    );
}

#[test]
#[cfg(all(feature = "semantic-triage", unix))]
fn build_delegates_to_active_watch() {
    use std::thread;

    use synrepo::pipeline::watch::{
        load_reconcile_state, request_watch_control, run_watch_service, watch_service_status,
        WatchConfig, WatchControlRequest, WatchControlResponse, WatchServiceMode,
        WatchServiceStatus,
    };

    let repo = tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(repo.path().join("src/lib.rs"), "pub fn greet() {}\n").unwrap();
    bootstrap(repo.path(), None, false).unwrap();
    let endpoint = spawn_embedding_server();
    enable_ollama_embeddings(repo.path(), &endpoint);

    let _watch_lock = synrepo::test_support::global_test_lock("watch-service");
    let _home_flock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let _home_guard = synrepo::config::test_home::lock_home_env_read();
    let config = Config::load(repo.path()).unwrap();
    let synrepo_dir = Config::synrepo_dir(repo.path());
    let service_repo = repo.path().to_path_buf();
    let service_config = config.clone();
    let service_synrepo = synrepo_dir.clone();

    let handle = thread::spawn(move || {
        run_watch_service(
            &service_repo,
            &service_config,
            &WatchConfig::default(),
            &service_synrepo,
            WatchServiceMode::Foreground,
            None,
        )
        .unwrap();
    });

    wait_for_watch(|| {
        matches!(
            watch_service_status(&synrepo_dir),
            WatchServiceStatus::Running(_)
        ) && load_reconcile_state(&synrepo_dir).is_ok()
    });

    let output = super::super::commands::embeddings_build_output(repo.path(), true).unwrap();
    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["status"], "completed");
    assert_eq!(value["summary"]["chunks"], 1);

    let stop = request_watch_control(&synrepo_dir, WatchControlRequest::Stop).unwrap();
    assert!(matches!(stop, WatchControlResponse::Ack { .. }));
    handle.join().unwrap();
}

#[cfg(feature = "semantic-triage")]
fn enable_ollama_embeddings(repo: &std::path::Path, endpoint: &str) {
    use synrepo::config::SemanticEmbeddingProvider;
    let path = Config::synrepo_dir(repo).join("config.toml");
    let mut config = Config::load(repo).unwrap();
    config.enable_semantic_triage = true;
    config.semantic_embedding_provider = SemanticEmbeddingProvider::Ollama;
    config.semantic_model = "fake-minilm".to_string();
    config.embedding_dim = 2;
    config.semantic_ollama_endpoint = endpoint.to_string();
    config.semantic_embedding_batch_size = 4;
    config.auto_sync_enabled = false;
    std::fs::write(path, toml::to_string_pretty(&config).unwrap()).unwrap();
}

#[cfg(feature = "semantic-triage")]
fn spawn_embedding_server() -> String {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(8).flatten() {
            respond(stream);
        }
    });
    format!("http://{addr}")
}

#[cfg(feature = "semantic-triage")]
fn respond(mut stream: std::net::TcpStream) {
    use std::io::{Read, Write};

    let mut request = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).unwrap();
        request.extend_from_slice(&chunk[..n]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let body = r#"{"embeddings":[[1.0,0.0]]}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).unwrap();
}

#[cfg(all(feature = "semantic-triage", unix))]
fn wait_for_watch(mut predicate: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("watch service did not become ready in time");
}
