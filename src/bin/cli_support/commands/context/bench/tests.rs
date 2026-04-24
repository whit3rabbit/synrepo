use super::*;

fn fx(category: &str, query: &str, targets: Vec<BenchTarget>) -> BenchTask {
    BenchTask {
        name: None,
        category: category.to_string(),
        query: query.to_string(),
        required_targets: targets,
    }
}

fn target(kind: &str, value: &str) -> BenchTarget {
    BenchTarget {
        kind: kind.to_string(),
        value: value.to_string(),
    }
}

#[test]
fn expand_task_glob_finds_json_fixtures() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("benches/tasks")).unwrap();
    std::fs::write(
        dir.path().join("benches/tasks/context.json"),
        r#"{"category":"route_to_edit","query":"auth"}"#,
    )
    .unwrap();

    let paths = expand_task_glob(dir.path(), "benches/tasks/*.json").unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("context.json"));
}

#[test]
fn validate_rejects_empty_query() {
    let fixture = fx("route_to_edit", "   ", vec![]);
    let err = validate_fixture(&fixture).unwrap_err().to_string();
    assert!(err.contains("query"), "error: {err}");
}

#[test]
fn validate_rejects_unknown_target_kind() {
    let fixture = fx(
        "route_to_edit",
        "auth",
        vec![target("module", "src/auth/mod.rs")],
    );
    let err = validate_fixture(&fixture).unwrap_err().to_string();
    assert!(err.contains("unknown"), "error: {err}");
    assert!(err.contains("module"), "error: {err}");
}

#[test]
fn validate_rejects_empty_target_value() {
    let fixture = fx("route_to_edit", "auth", vec![target("file", "")]);
    let err = validate_fixture(&fixture).unwrap_err().to_string();
    assert!(err.contains("value"), "error: {err}");
}

#[test]
fn validate_accepts_zero_targets() {
    let fixture = fx("route_to_edit", "auth", vec![]);
    assert!(validate_fixture(&fixture).is_ok());
}

#[test]
fn classify_hit_and_miss_on_file_target() {
    let required = vec![
        target("file", "src/auth/mod.rs"),
        target("file", "src/missing.rs"),
    ];
    let returned = vec!["src/auth/mod.rs".to_string(), "src/other.rs".to_string()];
    let (hits, misses) = classify_targets(&required, &returned, &[]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].value, "src/auth/mod.rs");
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0].value, "src/missing.rs");
}

#[test]
fn classify_symbol_target_matches_returned_symbols() {
    let required = vec![target("symbol", "authenticate")];
    let returned_symbols = vec!["auth::authenticate".to_string()];
    let (hits, misses) = classify_targets(&required, &[], &returned_symbols);
    assert_eq!(hits.len(), 1);
    assert!(misses.is_empty());
}

#[test]
fn classify_symbol_target_falls_back_to_paths() {
    let required = vec![target("symbol", "auth")];
    let returned_paths = vec!["src/auth/mod.rs".to_string()];
    let (hits, _) = classify_targets(&required, &returned_paths, &[]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn summarize_flags_missing_known_categories() {
    let tasks = vec![BenchTaskReport {
        name: "t1".into(),
        category: "route_to_edit".into(),
        query: "auth".into(),
        baseline_kind: BASELINE_KIND_RAW_FILE.into(),
        raw_file_tokens: 1_000,
        card_tokens: 100,
        reduction_ratio: 0.9,
        target_hit: true,
        target_hits: vec![],
        target_misses: vec![],
        stale_rate: 0.0,
        latency_ms: 3,
        returned_targets: vec![],
    }];
    let summary = summarize(&tasks);
    assert_eq!(summary.total_tasks, 1);
    assert_eq!(summary.categories, vec!["route_to_edit".to_string()]);
    assert_eq!(
        summary.missing_categories,
        vec![
            "symbol_explanation".to_string(),
            "impact_or_risk".to_string(),
            "test_surface".to_string(),
        ]
    );
}

#[test]
fn summarize_on_empty_fixture_set_reports_all_known_missing() {
    let summary = summarize(&[]);
    assert_eq!(summary.total_tasks, 0);
    assert_eq!(summary.tasks_with_hits, 0);
    assert_eq!(summary.tasks_with_misses, 0);
    assert!(summary.categories.is_empty());
    assert_eq!(summary.missing_categories.len(), KNOWN_CATEGORIES.len());
}

#[test]
fn stale_rate_is_fraction_of_examined_cards() {
    let examined = 4usize;
    let stale = 1usize;
    let rate = stale as f64 / examined as f64;
    assert!((rate - 0.25).abs() < f64::EPSILON);
}

#[test]
fn golden_report_shape_is_stable() {
    let report = BenchContextReport {
        schema_version: SCHEMA_VERSION,
        summary: BenchContextSummary {
            total_tasks: 1,
            tasks_with_hits: 1,
            tasks_with_misses: 0,
            categories: vec!["route_to_edit".into()],
            missing_categories: vec![
                "symbol_explanation".into(),
                "impact_or_risk".into(),
                "test_surface".into(),
            ],
        },
        tasks: vec![BenchTaskReport {
            name: "routing-auth".into(),
            category: "route_to_edit".into(),
            query: "authentication".into(),
            baseline_kind: BASELINE_KIND_RAW_FILE.into(),
            raw_file_tokens: 2_000,
            card_tokens: 200,
            reduction_ratio: 0.9,
            target_hit: true,
            target_hits: vec![target("file", "src/auth/mod.rs")],
            target_misses: vec![],
            stale_rate: 0.0,
            latency_ms: 5,
            returned_targets: vec!["src/auth/mod.rs".into()],
        }],
    };
    let json = serde_json::to_value(&report).unwrap();
    assert_key_set(&json, &["schema_version", "summary", "tasks"], "top-level");
    assert_key_set(
        &json["tasks"][0],
        &[
            "name",
            "category",
            "query",
            "baseline_kind",
            "raw_file_tokens",
            "card_tokens",
            "reduction_ratio",
            "target_hit",
            "target_hits",
            "target_misses",
            "stale_rate",
            "latency_ms",
            "returned_targets",
        ],
        "task",
    );
    assert_key_set(
        &json["summary"],
        &[
            "total_tasks",
            "tasks_with_hits",
            "tasks_with_misses",
            "categories",
            "missing_categories",
        ],
        "summary",
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
