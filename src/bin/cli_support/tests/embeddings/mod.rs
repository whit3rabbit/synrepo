mod incremental;

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
pub(super) fn enable_ollama_embeddings(repo: &std::path::Path, endpoint: &str) {
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
pub(super) struct MockServer {
    pub(super) endpoint: String,
    pub(super) call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub(super) bodies: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

#[cfg(feature = "semantic-triage")]
pub(super) fn spawn_recording_server() -> MockServer {
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let call_count = Arc::new(AtomicUsize::new(0));
    let bodies = Arc::new(Mutex::new(Vec::new()));

    let count_clone = Arc::clone(&call_count);
    let bodies_clone = Arc::clone(&bodies);

    std::thread::spawn(move || {
        for stream in listener.incoming().take(32).flatten() {
            count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            respond_recording(stream, &bodies_clone);
        }
    });

    MockServer {
        endpoint: format!("http://{addr}"),
        call_count,
        bodies,
    }
}

#[cfg(feature = "semantic-triage")]
fn spawn_embedding_server() -> String {
    spawn_recording_server().endpoint
}

#[cfg(feature = "semantic-triage")]
fn respond_recording(
    mut stream: std::net::TcpStream,
    bodies: &std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) {
    use std::io::{Read, Write};

    let mut request = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        request.extend_from_slice(&chunk[..n]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&request);
    let header_end = text.find("\r\n\r\n").unwrap_or(text.len());
    let (headers, body_part) = text.split_at(header_end);
    let mut body_bytes = body_part.trim_start_matches("\r\n\r\n").as_bytes().to_vec();

    let content_len = headers
        .lines()
        .find_map(|line| {
            let lower = line.to_lowercase();
            if lower.starts_with("content-length:") {
                lower["content-length:".len()..]
                    .trim()
                    .parse::<usize>()
                    .ok()
            } else {
                None
            }
        })
        .unwrap_or(0);

    while body_bytes.len() < content_len {
        let n = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        body_bytes.extend_from_slice(&chunk[..n]);
    }

    let body_str = String::from_utf8_lossy(&body_bytes).to_string();
    let num_inputs = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body_str) {
        if let Some(arr) = val["input"].as_array() {
            arr.len()
        } else {
            1
        }
    } else {
        1
    };

    bodies.lock().unwrap().push(body_str);

    let embeddings: Vec<Vec<f32>> = vec![vec![1.0, 0.0]; num_inputs];
    let resp_json = serde_json::json!({ "embeddings": embeddings }).to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        resp_json.len(),
        resp_json
    );
    let _ = stream.write_all(response.as_bytes());
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
