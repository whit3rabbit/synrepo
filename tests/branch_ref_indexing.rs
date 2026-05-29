use std::{fs, process::Command};

use synrepo::{
    bootstrap::bootstrap_with_force_and_config,
    config::{BranchRootsConfig, Config},
    pipeline::{git::branch_root_id, watch::run_reconcile_pass},
    store::sqlite::SqliteGraphStore,
    substrate::{discover, search_rooted_with_options, DiscoveryRootKind},
    surface::mcp::{
        edits::{handle_prepare_edit_context, PrepareEditContextParams},
        SynrepoState,
    },
};
use tempfile::tempdir;

fn git(repo: &tempfile::TempDir, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup_repo() -> tempfile::TempDir {
    let repo = tempdir().unwrap();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.name", "synrepo"]);
    git(&repo, &["config", "user.email", "synrepo@example.com"]);
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn main_only() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "main"]);
    git(&repo, &["checkout", "-b", "feature"]);
    fs::write(
        repo.path().join("src/branch.rs"),
        "pub fn branch_only_token() {}\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "feature"]);
    git(&repo, &["checkout", "main"]);
    repo
}

#[test]
fn branch_ref_cache_participates_in_search_and_graph() {
    let repo = setup_repo();
    let config = Config {
        branch_roots: BranchRootsConfig {
            refs: vec!["refs/heads/feature".to_string()],
            poll_seconds: 30,
        },
        ..Config::default()
    };
    bootstrap_with_force_and_config(repo.path(), None, false, false, |cfg| {
        cfg.branch_roots = config.branch_roots.clone();
    })
    .unwrap();

    let root_id = branch_root_id("refs/heads/feature");
    let cache_root = Config::synrepo_dir(repo.path())
        .join("branch-cache")
        .join(&root_id);
    assert!(cache_root.is_dir(), "branch cache root must exist");
    let cache_entries = fs::read_dir(&cache_root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let has_cached_branch_file = cache_entries.iter().any(|name| {
        cache_root
            .join(name)
            .join("src")
            .join("branch.rs")
            .is_file()
    });
    assert!(
        has_cached_branch_file,
        "branch file must be cached: {cache_entries:?}"
    );

    let discovered = discover(repo.path(), &config).unwrap();
    assert!(
        discovered
            .iter()
            .any(|file| file.relative_path == "src/branch.rs"
                && file.root_kind == DiscoveryRootKind::BranchRef),
        "branch snapshot file must be discoverable; cache entries: {cache_entries:?}"
    );

    let matches = search_rooted_with_options(
        &config,
        repo.path(),
        "branch_only_token",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(matches.len(), 1);
    let hit = &matches[0];
    assert_eq!(hit.root_kind, "branch_ref");
    assert_eq!(hit.root_ref.as_deref(), Some("refs/heads/feature"));
    assert!(!hit.editable);

    assert_eq!(hit.root_id, root_id);

    let graph = SqliteGraphStore::open(&Config::synrepo_dir(repo.path()).join("graph")).unwrap();
    let file = graph
        .file_by_root_path(&hit.root_id, "src/branch.rs")
        .unwrap()
        .expect("branch file must be graphed");
    assert_eq!(file.root_id, hit.root_id);

    let roots = synrepo::substrate::discover_roots(repo.path(), &config);
    let branch_root = roots
        .into_iter()
        .find(|root| root.discriminant == hit.root_id)
        .unwrap();
    assert_eq!(branch_root.kind, DiscoveryRootKind::BranchRef);
    assert!(!branch_root.editable);

    let state = SynrepoState {
        config: config.clone(),
        repo_root: repo.path().to_path_buf(),
    };
    let params: PrepareEditContextParams = serde_json::from_value(serde_json::json!({
        "target": "src/branch.rs",
        "target_kind": "file",
        "root_id": hit.root_id,
    }))
    .unwrap();
    let rejection: serde_json::Value =
        serde_json::from_str(&handle_prepare_edit_context(&state, params)).unwrap();
    assert_eq!(
        rejection["error"]["code"], "INVALID_PARAMETER",
        "{rejection}"
    );
    assert!(
        rejection["error"]["message"]
            .as_str()
            .unwrap()
            .contains("read-only branch_ref"),
        "{rejection}"
    );
}

#[test]
fn branch_ref_movement_keeps_root_id_and_refreshes_search() {
    let repo = setup_repo();
    let config = Config {
        branch_roots: BranchRootsConfig {
            refs: vec!["refs/heads/feature".to_string()],
            poll_seconds: 30,
        },
        ..Config::default()
    };
    bootstrap_with_force_and_config(repo.path(), None, false, false, |cfg| {
        cfg.branch_roots = config.branch_roots.clone();
    })
    .unwrap();

    git(&repo, &["checkout", "feature"]);
    fs::write(
        repo.path().join("src/branch.rs"),
        "pub fn branch_moved_token() {}\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "move feature"]);
    git(&repo, &["checkout", "main"]);

    let synrepo_dir = Config::synrepo_dir(repo.path());
    let outcome = run_reconcile_pass(repo.path(), &config, &synrepo_dir, false);
    assert!(matches!(
        outcome,
        synrepo::pipeline::watch::ReconcileOutcome::Completed(_)
    ));

    let root_id = branch_root_id("refs/heads/feature");
    let old = search_rooted_with_options(
        &config,
        repo.path(),
        "branch_only_token",
        &Default::default(),
    )
    .unwrap();
    assert!(old.is_empty());

    let new = search_rooted_with_options(
        &config,
        repo.path(),
        "branch_moved_token",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(new.len(), 1);
    assert_eq!(new[0].root_id, root_id);
    assert_eq!(new[0].root_kind, "branch_ref");
}
