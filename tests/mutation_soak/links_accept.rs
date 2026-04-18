use super::support::{
    assert_failure_contains, candidate_state, edge_count, run_ok, run_with_env,
    setup_curated_link_repo,
};

#[test]
#[ignore = "release-gate soak test; run with `cargo test --test mutation_soak -- --ignored --test-threads=1`"]
fn links_accept_survives_crash_after_pending_soak() {
    run_links_accept_crash_soak("links_accept:after_pending", "pending_promotion", 0);
}

#[test]
#[ignore = "release-gate soak test; run with `cargo test --test mutation_soak -- --ignored --test-threads=1`"]
fn links_accept_survives_crash_after_graph_insert_soak() {
    run_links_accept_crash_soak("links_accept:after_graph_insert", "pending_promotion", 1);
}

fn run_links_accept_crash_soak(failpoint: &str, expected_state: &str, expected_edges: usize) {
    for round in 0..10 {
        let fixture = setup_curated_link_repo(&format!("soak-pass-{round:02}"));
        let crash = run_with_env(
            &fixture.repo,
            &[
                "links",
                "accept",
                &fixture.candidate_id,
                "--reviewer",
                "soak-user",
            ],
            &[("SYNREPO_TEST_CRASH_AT", failpoint)],
        );
        assert_failure_contains(crash, "");
        assert_eq!(candidate_state(&fixture.repo, &fixture), expected_state);
        assert_eq!(edge_count(&fixture.repo, &fixture), expected_edges);

        let output = run_ok(
            &fixture.repo,
            &[
                "links",
                "accept",
                &fixture.candidate_id,
                "--reviewer",
                "soak-user",
            ],
        );
        assert!(
            output.contains("accepted and written to graph")
                || output.contains("promotion completed (crash recovery)")
        );
        assert_eq!(candidate_state(&fixture.repo, &fixture), "promoted");
        assert_eq!(edge_count(&fixture.repo, &fixture), 1);

        let replay = run_ok(
            &fixture.repo,
            &[
                "links",
                "accept",
                &fixture.candidate_id,
                "--reviewer",
                "soak-user",
            ],
        );
        assert!(replay.contains("already promoted"));
        assert_eq!(edge_count(&fixture.repo, &fixture), 1);
    }
}
