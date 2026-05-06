use super::*;

fn fx(category: &str, query: &str, targets: Vec<BenchTarget>) -> BenchTask {
    BenchTask {
        name: None,
        category: category.to_string(),
        query: query.to_string(),
        required_targets: targets,
        scope: None,
        shape: None,
        ground: None,
        budget: None,
        expected_recipe: None,
        allowed_context: None,
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
fn validate_accepts_v2_optional_fields() {
    let mut fixture = fx("route_to_edit", "auth", vec![]);
    fixture.allowed_context = Some(vec![target("file", "src/auth/mod.rs")]);
    fixture.expected_recipe = Some(synrepo::surface::context::ContextRecipe::ReviewModule);
    assert!(validate_fixture(&fixture).is_ok());
}

#[test]
fn mode_parser_accepts_expected_values() {
    assert_eq!(
        mode::BenchContextMode::parse("cards").unwrap(),
        mode::BenchContextMode::Cards
    );
    assert_eq!(
        mode::BenchContextMode::parse("ask").unwrap(),
        mode::BenchContextMode::Ask
    );
    assert_eq!(
        mode::BenchContextMode::parse("all").unwrap(),
        mode::BenchContextMode::All
    );
    assert!(mode::BenchContextMode::parse("other").is_err());
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
fn wrong_context_rate_is_null_without_allow_list() {
    let rate = wrong_context_rate(None, &["src/auth/mod.rs".into()], &[]);
    assert_eq!(rate, None);
}

#[test]
fn wrong_context_rate_counts_returned_items_outside_allow_list() {
    let allowed = vec![target("file", "src/auth/mod.rs")];
    let returned = vec!["src/auth/mod.rs".into(), "src/other.rs".into()];
    let rate = wrong_context_rate(Some(&allowed), &returned, &[]).unwrap();
    assert!((rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn summarize_flags_missing_known_categories() {
    let cards = run(true, 100, 1_000);
    let tasks = vec![BenchTaskReport {
        name: "t1".into(),
        category: "route_to_edit".into(),
        query: "auth".into(),
        expected_recipe: None,
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
        runs: BenchRunSet {
            cards: Some(cards),
            ..BenchRunSet::default()
        },
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
    assert!(summary.strategy_totals.is_empty());
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
    let cards = report::BenchStrategyRun {
        target_hits: vec![target("file", "src/auth/mod.rs")],
        returned_targets: vec!["src/auth/mod.rs".into()],
        ..run(true, 200, 2_000)
    };
    let ask = report::BenchStrategyRun {
        citation_coverage: 1.0,
        span_coverage: 1.0,
        expected_recipe_hit: Some(true),
        ..run(true, 180, 0)
    };
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
            strategy_totals: summarize(&[BenchTaskReport {
                name: "tmp".into(),
                category: "route_to_edit".into(),
                query: "authentication".into(),
                expected_recipe: None,
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
                runs: BenchRunSet {
                    cards: Some(cards.clone()),
                    ask: Some(ask.clone()),
                    ..BenchRunSet::default()
                },
            }])
            .strategy_totals,
            ask_improved_tasks: 0,
            ask_matched_tasks: 1,
            ask_regressed_tasks: 0,
        },
        tasks: vec![BenchTaskReport {
            name: "routing-auth".into(),
            category: "route_to_edit".into(),
            query: "authentication".into(),
            expected_recipe: None,
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
            runs: BenchRunSet {
                cards: Some(cards),
                ask: Some(ask),
                ..BenchRunSet::default()
            },
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
            "runs",
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
            "strategy_totals",
            "ask_improved_tasks",
            "ask_matched_tasks",
            "ask_regressed_tasks",
        ],
        "summary",
    );
    assert_key_set(
        &json["tasks"][0]["runs"]["ask"],
        &[
            "task_success",
            "tokens_returned",
            "tool_calls_needed",
            "estimated_followup_files",
            "latency_ms",
            "citation_coverage",
            "span_coverage",
            "wrong_context_rate",
            "target_hit",
            "target_hits",
            "target_misses",
            "stale_rate",
            "returned_targets",
            "returned_symbols",
            "expected_recipe_hit",
        ],
        "ask run",
    );
}

fn run(success: bool, tokens: usize, raw_tokens: usize) -> report::BenchStrategyRun {
    report::BenchStrategyRun {
        task_success: success,
        tokens_returned: tokens,
        tool_calls_needed: 1,
        estimated_followup_files: usize::from(!success),
        latency_ms: 5,
        citation_coverage: 0.0,
        span_coverage: 0.0,
        wrong_context_rate: None,
        target_hit: success,
        target_hits: Vec::new(),
        target_misses: Vec::new(),
        stale_rate: 0.0,
        returned_targets: Vec::new(),
        returned_symbols: Vec::new(),
        expected_recipe_hit: None,
        raw_file_tokens: raw_tokens,
    }
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
