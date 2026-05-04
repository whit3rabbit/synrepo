use super::target_finished_line;

#[test]
fn skipped_line_includes_budget_reason() {
    let line = target_finished_line(
        false,
        "src/lib.rs",
        Some("5888 est. tokens > 5000 budget"),
        0,
        false,
    );
    assert_eq!(line, "Skipped src/lib.rs: 5888 est. tokens > 5000 budget");
}

#[test]
fn skipped_line_includes_retry_and_queue_state() {
    let line = target_finished_line(
        false,
        "src/lib.rs",
        Some("non-success status: 429 Too Many Requests"),
        2,
        true,
    );
    assert!(line.contains("after 2 retry"));
    assert!(line.contains("429 Too Many Requests"));
    assert!(line.contains("(queued)"));
}
