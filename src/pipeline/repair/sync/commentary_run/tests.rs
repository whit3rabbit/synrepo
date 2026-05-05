use std::{
    collections::HashSet,
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use tempfile::{tempdir, TempDir};
use time::OffsetDateTime;

use super::*;
use crate::{
    core::{
        ids::{FileNodeId, NodeId},
        provenance::{Provenance, SourceRef},
    },
    overlay::{CommentaryEntry, CommentaryProvenance, OverlayStore},
    pipeline::explain::{
        CommentaryFuture, CommentaryGeneration, CommentaryGenerator, CommentarySkip,
    },
    pipeline::repair::CommentaryWorkPhase,
    structure::graph::{Epistemic, FileNode, GraphStore},
};

#[test]
fn concurrent_phase_caps_provider_calls_and_persists_hash() {
    let mut fixture = Fixture::new(5);
    let generator = Arc::new(DelayGenerator::new(Duration::from_millis(25)));
    let stats = run_fixture_phase(&mut fixture, generator.clone(), 2, RunPhase::FileSeed);

    assert_eq!(stats.attempted, 5);
    assert_eq!(stats.generated, 5);
    assert_eq!(generator.max_active.load(Ordering::SeqCst), 2);
    for (idx, item) in fixture.items.iter().enumerate() {
        let entry = fixture
            .overlay
            .commentary_for(item.node_id)
            .unwrap()
            .expect("commentary persisted");
        assert_eq!(entry.provenance.source_content_hash, format!("hash-{idx}"));
    }
}

#[test]
fn concurrency_one_uses_serial_generator_path() {
    let mut fixture = Fixture::new(3);
    let generator = Arc::new(DelayGenerator::new(Duration::ZERO));
    let stats = run_fixture_phase(&mut fixture, generator.clone(), 1, RunPhase::FileSeed);

    assert_eq!(stats.attempted, 3);
    assert_eq!(stats.generated, 3);
    assert_eq!(generator.sync_calls.load(Ordering::SeqCst), 3);
    assert_eq!(generator.async_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn rate_limit_halts_new_scheduling_but_drains_in_flight() {
    let mut fixture = Fixture::new(5);
    let limited = fixture.items[0].node_id;
    let in_flight = fixture.items[1].node_id;
    let unscheduled = fixture.items[2].node_id;
    let generator = Arc::new(RateLimitFirst {
        limited,
        limited_calls: AtomicUsize::new(0),
    });

    let mut totals = RunTotals::default();
    let mut progress = None;
    let mut should_stop = None;
    let mut commented = HashSet::new();
    let items = fixture.items.clone();
    let mut executor = fixture.executor(generator.clone(), 2);
    let stats = run_phase(
        &mut executor,
        &mut progress,
        &mut should_stop,
        &mut totals,
        &items,
        RunPhase::FileSeed,
        &mut commented,
    )
    .unwrap();

    assert_eq!(stats.attempted, 2);
    assert_eq!(stats.generated, 1);
    assert!(totals.halted_for_rate_limit);
    assert_eq!(generator.limited_calls.load(Ordering::SeqCst), 3);
    assert!(fixture.overlay.commentary_for(in_flight).unwrap().is_some());
    assert!(fixture
        .overlay
        .commentary_for(unscheduled)
        .unwrap()
        .is_none());
}

struct Fixture {
    _repo: TempDir,
    graph: SqliteGraphStore,
    overlay: SqliteOverlayStore,
    items: Vec<CommentaryWorkItem>,
}

impl Fixture {
    fn new(count: usize) -> Self {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        let graph_dir = repo.path().join(".synrepo/graph");
        let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
        let mut items = Vec::new();

        graph.begin().unwrap();
        for idx in 0..count {
            let path = format!("src/file_{idx}.rs");
            fs::write(repo.path().join(&path), format!("fn file_{idx}() {{}}\n")).unwrap();
            let file = file_node(idx, &path);
            graph.upsert_file(file.clone()).unwrap();
            items.push(CommentaryWorkItem {
                node_id: NodeId::File(file.id),
                file_id: file.id,
                phase: CommentaryWorkPhase::Seed,
                path,
                qualified_name: None,
            });
        }
        graph.commit().unwrap();

        let overlay = SqliteOverlayStore::open(&repo.path().join(".synrepo/overlay")).unwrap();
        Self {
            _repo: repo,
            graph,
            overlay,
            items,
        }
    }

    fn executor(
        &mut self,
        generator: Arc<dyn CommentaryGenerator>,
        concurrency: usize,
    ) -> ItemExecutor<'_> {
        ItemExecutor {
            repo_root: self._repo.path(),
            graph: &self.graph,
            overlay: &mut self.overlay,
            generator,
            max_input_tokens: 5000,
            max_targets: self.items.len(),
            concurrency,
        }
    }
}

fn run_fixture_phase(
    fixture: &mut Fixture,
    generator: Arc<dyn CommentaryGenerator>,
    concurrency: usize,
    phase: RunPhase,
) -> PhaseStats {
    let items = fixture.items.clone();
    let mut totals = RunTotals::default();
    let mut progress = None;
    let mut should_stop = None;
    let mut commented = HashSet::new();
    let mut executor = fixture.executor(generator, concurrency);
    run_phase(
        &mut executor,
        &mut progress,
        &mut should_stop,
        &mut totals,
        &items,
        phase,
        &mut commented,
    )
    .unwrap()
}

struct DelayGenerator {
    delay: Duration,
    active: AtomicUsize,
    max_active: AtomicUsize,
    sync_calls: AtomicUsize,
    async_calls: AtomicUsize,
}

impl DelayGenerator {
    fn new(delay: Duration) -> Self {
        Self {
            delay,
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            sync_calls: AtomicUsize::new(0),
            async_calls: AtomicUsize::new(0),
        }
    }

    fn enter(&self) {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
    }

    fn exit(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

impl CommentaryGenerator for DelayGenerator {
    fn generate(&self, node: NodeId, _context: &str) -> crate::Result<Option<CommentaryEntry>> {
        self.sync_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(entry(node, "serial")))
    }

    fn generate_with_outcome_async<'a>(
        &'a self,
        node: NodeId,
        _context: &'a str,
    ) -> CommentaryFuture<'a> {
        Box::pin(async move {
            self.async_calls.fetch_add(1, Ordering::SeqCst);
            self.enter();
            tokio::time::sleep(self.delay).await;
            self.exit();
            Ok(CommentaryGeneration::Generated(entry(node, "async")))
        })
    }
}

struct RateLimitFirst {
    limited: NodeId,
    limited_calls: AtomicUsize,
}

impl CommentaryGenerator for RateLimitFirst {
    fn generate(&self, node: NodeId, _context: &str) -> crate::Result<Option<CommentaryEntry>> {
        Ok(Some(entry(node, "serial fallback")))
    }

    fn generate_with_outcome_async<'a>(
        &'a self,
        node: NodeId,
        _context: &'a str,
    ) -> CommentaryFuture<'a> {
        Box::pin(async move {
            if node == self.limited {
                self.limited_calls.fetch_add(1, Ordering::SeqCst);
                return Ok(CommentaryGeneration::Skipped(CommentarySkip::rate_limited(
                    "rate limited",
                    Some(Duration::ZERO),
                )));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(CommentaryGeneration::Generated(entry(node, "generated")))
        })
    }
}

fn entry(node: NodeId, text: &str) -> CommentaryEntry {
    CommentaryEntry {
        node_id: node,
        text: text.to_string(),
        provenance: CommentaryProvenance {
            source_content_hash: String::new(),
            pass_id: "test".to_string(),
            model_identity: "fixture".to_string(),
            generated_at: OffsetDateTime::now_utc(),
        },
    }
}

fn file_node(idx: usize, path: &str) -> FileNode {
    let id = FileNodeId((idx + 1) as u128);
    FileNode {
        id,
        root_id: "primary".to_string(),
        path: path.to_string(),
        path_history: Vec::new(),
        content_hash: format!("hash-{idx}"),
        content_sample_hashes: Vec::new(),
        size_bytes: 1,
        language: Some("rust".to_string()),
        inline_decisions: Vec::new(),
        last_observed_rev: None,
        epistemic: Epistemic::ParserObserved,
        provenance: Provenance::structural(
            "test",
            "rev",
            vec![SourceRef {
                file_id: Some(id),
                path: path.to_string(),
                content_hash: format!("hash-{idx}"),
            }],
        ),
    }
}
