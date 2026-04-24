use std::fs;
use tempfile::tempdir;

use super::support::{init_empty_graph, seed_files};
use crate::config::Config;
use crate::pipeline::export::{write_exports, ExportFormat};
use crate::store::sqlite::SqliteGraphStore;
use crate::surface::card::Budget;

#[test]
fn export_streams_large_file_set_without_batch_materialization() {
    // Regression guard: with streaming, peak memory is one card at a time; this
    // test asserts the render pipeline still completes and emits every card
    // when the graph contains many files. Memory profiling is manual; the test
    // value is catching accidental re-introduction of Vec<Card> materialization
    // that would only surface at scale.
    const N: usize = 100;
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    seed_files(&synrepo_dir, N);

    let config = Config {
        export_dir: "large-json".to_string(),
        ..Config::default()
    };

    let result = write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Json,
        Budget::Normal,
        true,
    )
    .unwrap();

    assert_eq!(result.file_count, N, "every seeded file must be rendered");

    let raw = fs::read_to_string(repo.path().join("large-json").join("index.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("JSON must round-trip");
    let files = parsed
        .get("files")
        .and_then(|v| v.as_array())
        .expect("files array");
    assert_eq!(files.len(), N);
}

#[test]
fn export_json_is_well_formed_with_empty_collections() {
    // The manual array-bracket framing in render::write_json must still emit
    // parseable JSON when every collection is empty (no stray commas, no
    // open-but-never-closed brackets).
    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    init_empty_graph(&synrepo_dir).unwrap();

    let config = Config {
        export_dir: "empty-json".to_string(),
        ..Config::default()
    };

    write_exports(
        repo.path(),
        &synrepo_dir,
        &config,
        ExportFormat::Json,
        Budget::Normal,
        true,
    )
    .unwrap();

    let raw = fs::read_to_string(repo.path().join("empty-json").join("index.json")).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).expect("empty-export JSON must round-trip through serde_json");

    let obj = parsed
        .as_object()
        .expect("top-level JSON must be an object");
    assert!(obj.contains_key("generated_note"));
    assert!(obj.contains_key("change_risk"));
    for key in ["files", "symbols", "decisions"] {
        let arr = obj
            .get(key)
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{key} must be a JSON array"));
        assert!(arr.is_empty(), "{key} must be empty for empty graph");
    }
}

/// `io::Write` wrapper that tracks peak single-write size.
///
/// The regression this catches: a change that batch-materializes all cards
/// into one string (`serde_json::to_string(&all)` + `write_all`) hands the
/// entire blob to the writer in a single call. The streaming path routes
/// each card through `serde_json::to_writer`, which emits many small
/// incremental writes per field. Peak single-write size is therefore
/// O(one card's largest field) under streaming vs. O(all cards) under a
/// batch regression — an easy, deterministic discriminator.
struct CountingWriter {
    peak_single_write: usize,
    total_bytes: usize,
}

impl CountingWriter {
    fn new() -> Self {
        Self {
            peak_single_write: 0,
            total_bytes: 0,
        }
    }
}

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.total_bytes += buf.len();
        if buf.len() > self.peak_single_write {
            self.peak_single_write = buf.len();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn export_json_peak_in_flight_bytes_stays_under_budget() {
    use crate::pipeline::export::render;
    use crate::surface::card::{compiler::GraphCardCompiler, CardCompiler};

    // 500 seeded files is well past the point where Vec<Card> materialization
    // would show up as a single large write call under the batch regression.
    const N: usize = 500;
    // Under streaming, `serde_json::to_writer` emits many small incremental
    // writes per card field (identifiers, numbers, short strings, struct
    // delimiters), so the largest single write is on the order of tens of
    // bytes. 64 KiB leaves ample headroom above today's per-write size and
    // still traps any regression that buffers all cards into one blob and
    // delivers them in a single write_all call (that would be hundreds of
    // KB for N=500 at current card size).
    const SINGLE_WRITE_BUDGET: usize = 64 * 1024;

    let repo = tempdir().unwrap();
    let synrepo_dir = repo.path().join(".synrepo");
    seed_files(&synrepo_dir, N);

    let graph = SqliteGraphStore::open_existing(&synrepo_dir.join("graph")).unwrap();
    let config = Config::default();
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_config(config);

    let (file_ids, symbol_ids) = compiler
        .with_reader(|g| {
            let fids: Vec<_> = g
                .all_file_paths()
                .unwrap()
                .into_iter()
                .map(|(_, id)| id)
                .collect();
            let sids: Vec<_> = g
                .all_symbol_names()
                .unwrap()
                .into_iter()
                .map(|(id, _, _)| id)
                .collect();
            Ok((fids, sids))
        })
        .unwrap();

    assert_eq!(file_ids.len(), N, "seeded files must appear in graph");

    let file_stream = file_ids
        .iter()
        .filter_map(|id| compiler.file_card(*id, Budget::Normal).ok());
    let symbol_stream = symbol_ids
        .iter()
        .filter_map(|id| compiler.symbol_card(*id, Budget::Normal).ok());
    let decision_stream: Vec<crate::pipeline::export::ExportDecision> = Vec::new();

    let mut counting = CountingWriter::new();
    let (files, _, _) =
        render::write_json_to_writer(&mut counting, file_stream, symbol_stream, decision_stream)
            .expect("write_json_to_writer must succeed");

    assert_eq!(files, N, "every seeded file must be rendered");
    assert!(
        counting.total_bytes > 0,
        "peak-budget assertion is only meaningful if the writer actually saw bytes"
    );
    assert!(
        counting.peak_single_write <= SINGLE_WRITE_BUDGET,
        "streaming JSON export exceeded single-write budget: \
         peak_single_write={peak} bytes, budget={budget} bytes, total={total} bytes, files={files}. \
         A regression that batch-materializes Vec<Card> and writes the whole buffer in one call \
         would trip this assertion.",
        peak = counting.peak_single_write,
        budget = SINGLE_WRITE_BUDGET,
        total = counting.total_bytes,
        files = files,
    );
}
