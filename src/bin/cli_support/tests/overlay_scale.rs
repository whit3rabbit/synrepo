//! Scale regression guards for overlay-candidate consumers.
//!
//! Today the overlay reader surfaces (handoffs, status, `links list`) are
//! bounded either by per-query LIMIT clauses or by small fixtures. Nothing
//! catches a future refactor that reintroduces O(N²) behavior — for example,
//! replacing a SQL LIMIT with an in-memory filter, or adding a per-row
//! N+1 query inside the collect loop. These tests seed a realistic-large
//! candidate set and pin a wall-clock ceiling on each reader path.
//!
//! The timing bounds are intentionally generous: they exist to catch
//! order-of-magnitude regressions, not to enforce tight SLAs. If CI hardware
//! proves flaky, raise the bounds — do not delete the assertion and do not
//! trim the fixture size below the point where the guard is meaningful.

use std::time::{Duration, Instant};

use synrepo::config::Config;
use synrepo::surface::handoffs::read_pending_candidates;
use tempfile::tempdir;

use super::super::commands::{links_list_output, status_output};
use super::support::{bootstrap_isolated as bootstrap, seed_overlay_candidates};

const SCALE_N: u64 = 5_000;

/// Seeding this many candidates via auto-commit inserts is not free; tune
/// the ceiling per test with this budget. Seeding itself is not measured.
const READ_BUDGET: Duration = Duration::from_secs(10);

#[test]
fn read_pending_candidates_scales_to_5000_rows() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    seed_overlay_candidates(repo, SCALE_N);

    let overlay_dir = Config::synrepo_dir(repo).join("overlay");

    let start = Instant::now();
    let items = read_pending_candidates(&overlay_dir).expect("must not error on large input");
    let elapsed = start.elapsed();

    assert_eq!(
        items.len(),
        SCALE_N as usize,
        "every seeded candidate must appear as a handoff item"
    );
    assert!(
        elapsed < READ_BUDGET,
        "read_pending_candidates took {elapsed:?}; regression guard budget is {READ_BUDGET:?}"
    );
}

#[test]
fn status_default_with_5000_candidates_completes_quickly() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    bootstrap(repo, None, false).unwrap();
    seed_overlay_candidates(repo, SCALE_N);

    // Default path (no --full, no --recent): must stay cheap even when
    // overlay is large. Counterpart to the 1000-commentary timing test in
    // status.rs — that one guards the commentary coverage path; this one
    // guards the overlay-cost-summary path.
    let start = Instant::now();
    let output = status_output(repo, false, false, false).expect("status must not error");
    let elapsed = start.elapsed();

    assert!(
        elapsed < READ_BUDGET,
        "default status_output took {elapsed:?}; regression guard budget is {READ_BUDGET:?}"
    );
    // The timing check alone would pass for a no-op implementation. Pin that
    // the overlay path was actually traversed: default status must reach the
    // overlay-cost line that this test is guarding.
    assert!(
        output.contains("overlay cost:"),
        "expected default status to render the overlay-cost line, got: {output}"
    );
}

#[test]
fn links_list_with_5000_candidates_paginates() {
    let dir = tempdir().unwrap();
    let repo = dir.path();
    seed_overlay_candidates(repo, SCALE_N);

    // Default limit (None) applies LINKS_LIST_DEFAULT_LIMIT=50 at the SQL
    // layer, so the output must never materialize all 5000 rows.
    let start = Instant::now();
    let output = links_list_output(repo, None, None, false).expect("links list must not error");
    let elapsed = start.elapsed();

    assert!(
        elapsed < READ_BUDGET,
        "links_list_output with default limit took {elapsed:?}; budget is {READ_BUDGET:?}"
    );
    assert!(
        output.contains("capped at 50"),
        "default links list must print the capped-at-50 banner when more rows exist; got:\n{output}"
    );

    // `--limit 0` opts out of the cap. This path does materialize every row;
    // it exists for scripted exports. Budget is still the regression ceiling.
    let start = Instant::now();
    let output_all =
        links_list_output(repo, None, Some(0), false).expect("links list --limit 0 must not error");
    let elapsed_all = start.elapsed();

    assert!(
        elapsed_all < READ_BUDGET,
        "links_list_output with --limit 0 took {elapsed_all:?}; budget is {READ_BUDGET:?}"
    );
    assert!(
        output_all.contains(&format!("Found {SCALE_N} candidates.")),
        "unlimited links list must report the full count; got:\n{output_all}"
    );
}
