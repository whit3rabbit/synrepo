use super::*;

fn run(hit: bool, latency_ms: u64, semantic_available: bool) -> BenchSearchRun {
    BenchSearchRun {
        target_hit: hit,
        target_hits: Vec::new(),
        target_misses: Vec::new(),
        returned_targets: Vec::new(),
        returned_symbols: Vec::new(),
        latency_ms,
        engine: "syntext".to_string(),
        semantic_available,
        semantic_row_count: 0,
    }
}

#[test]
fn mode_parser_accepts_expected_values() {
    assert_eq!(
        BenchSearchMode::parse("lexical").unwrap(),
        BenchSearchMode::Lexical
    );
    assert_eq!(
        BenchSearchMode::parse("auto").unwrap(),
        BenchSearchMode::Auto
    );
    assert_eq!(
        BenchSearchMode::parse("both").unwrap(),
        BenchSearchMode::Both
    );
    assert!(BenchSearchMode::parse("other").is_err());
}

#[test]
fn summary_reports_hybrid_improvements_and_regressions() {
    let tasks = vec![
        BenchSearchTaskReport {
            name: "improved".into(),
            category: "route_to_edit".into(),
            query: "q1".into(),
            lexical: Some(run(false, 10, false)),
            auto: Some(run(true, 20, true)),
        },
        BenchSearchTaskReport {
            name: "regressed".into(),
            category: "route_to_edit".into(),
            query: "q2".into(),
            lexical: Some(run(true, 5, false)),
            auto: Some(run(false, 8, false)),
        },
    ];
    let summary = summarize(&tasks);
    assert_eq!(summary.total_tasks, 2);
    assert_eq!(summary.hybrid_improved_tasks, 1);
    assert_eq!(summary.hybrid_regressed_tasks, 1);
    assert_eq!(summary.semantic_available_tasks, 1);
    assert_eq!(summary.lexical_latency_ms, Some(15));
    assert_eq!(summary.auto_latency_ms, Some(28));
}

#[test]
fn golden_report_shape_is_stable() {
    let report = BenchSearchReport {
        schema_version: SCHEMA_VERSION,
        summary: BenchSearchSummary {
            total_tasks: 1,
            lexical_hit_at_5: Some(1.0),
            auto_hit_at_5: Some(1.0),
            lexical_latency_ms: Some(2),
            auto_latency_ms: Some(3),
            semantic_available_tasks: 1,
            hybrid_improved_tasks: 0,
            hybrid_matched_tasks: 1,
            hybrid_regressed_tasks: 0,
        },
        tasks: vec![BenchSearchTaskReport {
            name: "fixture".into(),
            category: "route_to_edit".into(),
            query: "find routing".into(),
            lexical: Some(run(true, 2, false)),
            auto: Some(run(true, 3, true)),
        }],
    };
    let json = serde_json::to_value(&report).unwrap();
    assert_key_set(&json, &["schema_version", "summary", "tasks"], "top-level");
    assert_key_set(
        &json["summary"],
        &[
            "total_tasks",
            "lexical_hit_at_5",
            "auto_hit_at_5",
            "lexical_latency_ms",
            "auto_latency_ms",
            "semantic_available_tasks",
            "hybrid_improved_tasks",
            "hybrid_matched_tasks",
            "hybrid_regressed_tasks",
        ],
        "summary",
    );
    assert_key_set(
        &json["tasks"][0]["auto"],
        &[
            "target_hit",
            "target_hits",
            "target_misses",
            "returned_targets",
            "returned_symbols",
            "latency_ms",
            "engine",
            "semantic_available",
            "semantic_row_count",
        ],
        "run",
    );
}

fn assert_key_set(value: &serde_json::Value, expected: &[&str], scope: &str) {
    use std::collections::BTreeSet;
    let actual: BTreeSet<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("{scope}: expected JSON object"))
        .keys()
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(actual, want, "{scope} key set mismatch");
}
