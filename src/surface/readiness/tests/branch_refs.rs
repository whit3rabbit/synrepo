use crate::surface::{
    branch_refs::{BranchRefStatus, BranchRootsStatus},
    readiness::{branch_refs::branch_refs_row_from_status, ReadinessState},
};

#[test]
fn branch_refs_row_reports_disabled_when_unconfigured() {
    let status = BranchRootsStatus::empty(30, false);

    let row = branch_refs_row_from_status(&status);

    assert_eq!(row.state, ReadinessState::Disabled);
    assert_eq!(row.detail, "no branch refs configured");
    assert!(row.next_action.is_none());
}

#[test]
fn branch_refs_row_reports_supported_and_monitoring_detail() {
    let status =
        BranchRootsStatus::from_refs(30, true, vec![BranchRefStatus::tracked("refs/heads/main")]);

    let row = branch_refs_row_from_status(&status);

    assert_eq!(row.state, ReadinessState::Supported);
    assert_eq!(row.detail, "1/1 tracked; monitored @30s");
    assert!(row.next_action.is_none());
}

#[test]
fn branch_refs_row_reports_stale_snapshots_or_indexes() {
    let status = BranchRootsStatus::from_refs(
        30,
        false,
        vec![
            BranchRefStatus::tracked("refs/heads/main"),
            BranchRefStatus::stale("refs/heads/release"),
        ],
    );

    let row = branch_refs_row_from_status(&status);

    assert_eq!(row.state, ReadinessState::Stale);
    assert_eq!(row.next_action.as_deref(), Some("run `synrepo reconcile`"));
}

#[test]
fn branch_refs_row_reports_missing_local_refs() {
    let status = BranchRootsStatus::from_refs(
        30,
        true,
        vec![
            BranchRefStatus::tracked("refs/heads/main"),
            BranchRefStatus::missing("refs/heads/release"),
        ],
    );

    let row = branch_refs_row_from_status(&status);

    assert_eq!(row.state, ReadinessState::Degraded);
    assert!(
        row.next_action
            .as_deref()
            .is_some_and(|action| action.contains("fetch missing refs")),
        "{row:?}"
    );
}
