use std::fs;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use super::super::run_structural_compile;
use super::support::open_graph;
use crate::{
    config::Config,
    structure::graph::{snapshot, Graph, GraphReader},
};
use tempfile::tempdir;
use tracing::subscriber::set_default;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct TestLogBuffer {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl TestLogBuffer {
    fn contents(&self) -> String {
        String::from_utf8(self.bytes.lock().unwrap().clone()).unwrap()
    }
}

struct TestLogWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestLogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for TestLogBuffer {
    type Writer = TestLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TestLogWriter {
            bytes: Arc::clone(&self.bytes),
        }
    }
}

#[test]
fn structural_compile_publishes_graph_snapshot() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn publish_me() {}\n").unwrap();

    snapshot::publish(repo.path(), Graph::empty());

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let published = snapshot::current(repo.path()).expect("snapshot should be published");
    assert!(published.snapshot_epoch > 0);
    assert_eq!(published.files.len(), 1);
    assert!(published.file_by_path("src/lib.rs").unwrap().is_some());
}

#[test]
fn structural_compile_skips_snapshot_when_it_exceeds_memory_ceiling() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn too_big() {}\n").unwrap();

    snapshot::publish(repo.path(), Graph::empty());

    let logs = TestLogBuffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(logs.clone())
        .finish();
    let _guard = set_default(subscriber);

    let config = Config {
        max_graph_snapshot_bytes: 1,
        ..Config::default()
    };
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    assert!(
        snapshot::current(repo.path()).is_none(),
        "stale snapshot must be evicted when replacement compile exceeds memory ceiling"
    );
    assert!(logs.contents().contains("skipping snapshot publication"));
}

#[test]
fn structural_compile_second_no_change_compile_leaves_published_epoch_untouched() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn stable() {}\n").unwrap();

    let config = Config::default();
    let mut graph = open_graph(&repo);
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let first = snapshot::current(repo.path()).expect("snapshot published on first run");
    let first_epoch = first.snapshot_epoch;
    assert!(first_epoch > 0);

    // Second compile with no file modifications
    run_structural_compile(repo.path(), &config, &mut graph).unwrap();

    let second = snapshot::current(repo.path()).expect("snapshot still present");
    assert_eq!(second.snapshot_epoch, first_epoch);
    assert!(Arc::ptr_eq(&first, &second));

    snapshot::forget(repo.path());
}
