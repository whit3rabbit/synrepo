use std::fs;

use super::support::{
    assert_failure_contains, candidate_state, compact_state_path, edge_count, export_manifest_path,
    hold_foreign_writer_lock, read_optional, run, run_ok, setup_curated_link_repo,
    write_upgrade_drift,
};

#[test]
#[ignore = "release-gate soak test; run with `cargo test --test mutation_soak -- --ignored --test-threads=1`"]
fn mutation_commands_fail_fast_under_writer_lock_and_recover_after_release() {
    let fixture = setup_curated_link_repo("writer-lock-soak");
    let repo_path = fixture.repo.as_path();
    write_upgrade_drift(repo_path);

    let export_dir = repo_path.join("synrepo-context");
    fs::create_dir_all(&export_dir).expect("create export dir");
    let sentinel = export_dir.join("sentinel.txt");
    fs::write(&sentinel, "writer-lock sentinel").expect("write sentinel");

    let compact_before = read_optional(&compact_state_path(repo_path));
    let manifest = export_manifest_path(repo_path);

    let (mut child, holder, pid) = hold_foreign_writer_lock(repo_path);

    for round in 0..25 {
        let output = match round % 6 {
            0 => run(repo_path, &["reconcile"]),
            1 => run(repo_path, &["sync"]),
            2 => run(repo_path, &["export"]),
            3 => run(repo_path, &["compact", "--apply"]),
            4 => run(repo_path, &["upgrade", "--apply"]),
            _ => run(
                repo_path,
                &[
                    "links",
                    "accept",
                    &fixture.candidate_id,
                    "--reviewer",
                    "soak-user",
                ],
            ),
        };
        assert_failure_contains(output, &format!("writer lock held by pid {pid}"));
        assert_eq!(candidate_state(repo_path, &fixture), "active");
        assert_eq!(edge_count(repo_path, &fixture), 0);
        assert_eq!(
            fs::read_to_string(&sentinel).expect("read sentinel"),
            "writer-lock sentinel"
        );
        assert!(
            !manifest.exists(),
            "blocked export must not write a manifest"
        );
        assert_eq!(
            read_optional(&compact_state_path(repo_path)),
            compact_before
        );
    }

    drop(holder);
    let _ = child.kill();
    let _ = child.wait();

    let accept = run_ok(
        repo_path,
        &[
            "links",
            "accept",
            &fixture.candidate_id,
            "--reviewer",
            "soak-user",
        ],
    );
    assert!(accept.contains("accepted and written to graph"));
    assert_eq!(candidate_state(repo_path, &fixture), "promoted");
    assert_eq!(edge_count(repo_path, &fixture), 1);

    let _ = run_ok(repo_path, &["reconcile"]);

    let _ = run_ok(repo_path, &["sync"]);

    let export = run_ok(repo_path, &["export"]);
    assert!(export.contains("Context export complete"));
    assert!(
        manifest.exists(),
        "export should write a manifest after recovery"
    );

    let compact = run_ok(repo_path, &["compact", "--apply"]);
    assert!(compact.contains("Compaction complete"));

    let upgrade = run_ok(repo_path, &["upgrade", "--apply"]);
    assert!(
        upgrade.contains("Upgrade complete.") || upgrade.contains("All stores are compatible.")
    );
}
