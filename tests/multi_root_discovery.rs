use std::{fs, path::Path, process::Command};

use synrepo::{
    config::Config,
    pipeline::structural::run_structural_compile,
    store::sqlite::SqliteGraphStore,
    substrate::{discover, DiscoveryRootKind},
};
use tempfile::tempdir;

#[test]
fn discovery_includes_linked_worktrees_when_enabled() {
    let fixture = worktree_fixture();

    let discovered = discover(fixture.main.path(), &Config::default()).unwrap();
    assert!(discovered
        .iter()
        .any(|file| file.root_kind == DiscoveryRootKind::Primary
            && file.relative_path == "src/lib.rs"));
    assert!(discovered
        .iter()
        .any(|file| file.root_kind == DiscoveryRootKind::Worktree
            && file.relative_path == "src/wt_only.rs"));

    let config = Config {
        include_worktrees: false,
        ..Config::default()
    };
    let without_worktrees = discover(fixture.main.path(), &config).unwrap();
    assert!(without_worktrees
        .iter()
        .all(|file| file.root_kind != DiscoveryRootKind::Worktree));
}

#[test]
fn discovery_includes_submodules_only_when_enabled() {
    let fixture = submodule_fixture();

    let without_submodules = discover(fixture.main.path(), &Config::default()).unwrap();
    assert!(without_submodules
        .iter()
        .all(|file| file.root_kind != DiscoveryRootKind::Submodule));

    let config = Config {
        include_submodules: true,
        include_worktrees: false,
        ..Config::default()
    };
    let with_submodules = discover(fixture.main.path(), &config).unwrap();
    assert!(with_submodules
        .iter()
        .any(|file| file.root_kind == DiscoveryRootKind::Submodule
            && file.relative_path == "src/sub.rs"));
}

#[test]
fn same_content_in_two_worktrees_gets_distinct_file_ids() {
    let fixture = worktree_fixture();
    fs::write(
        fixture.worktree.path().join("src/lib.rs"),
        "pub fn shared() {}\n",
    )
    .unwrap();
    fs::write(
        fixture.main.path().join("src/lib.rs"),
        "pub fn shared() {}\n",
    )
    .unwrap();

    let mut graph = SqliteGraphStore::open(&fixture.synrepo_graph()).unwrap();
    run_structural_compile(fixture.main.path(), &Config::default(), &mut graph).unwrap();

    let ids = graph
        .all_file_paths()
        .unwrap()
        .into_iter()
        .filter_map(|(path, file_id)| (path == "src/lib.rs").then_some(file_id))
        .collect::<Vec<_>>();

    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn rename_within_one_worktree_preserves_file_id() {
    let fixture = worktree_fixture();
    fs::write(
        fixture.worktree.path().join("src/wt_old.rs"),
        "pub fn stable_identity() {}\n",
    )
    .unwrap();

    let config = Config::default();
    let worktree_root = discover(fixture.main.path(), &config)
        .unwrap()
        .into_iter()
        .find(|file| file.root_kind == DiscoveryRootKind::Worktree)
        .unwrap()
        .root_discriminant;

    let mut graph = SqliteGraphStore::open(&fixture.synrepo_graph()).unwrap();
    run_structural_compile(fixture.main.path(), &config, &mut graph).unwrap();
    let before = graph
        .file_by_root_path(&worktree_root, "src/wt_old.rs")
        .unwrap()
        .unwrap()
        .id;

    fs::rename(
        fixture.worktree.path().join("src/wt_old.rs"),
        fixture.worktree.path().join("src/wt_new.rs"),
    )
    .unwrap();
    run_structural_compile(fixture.main.path(), &config, &mut graph).unwrap();

    let after = graph
        .file_by_root_path(&worktree_root, "src/wt_new.rs")
        .unwrap()
        .unwrap()
        .id;
    assert_eq!(before, after);
}

struct WorktreeFixture {
    main: tempfile::TempDir,
    worktree: tempfile::TempDir,
}

impl WorktreeFixture {
    fn synrepo_graph(&self) -> std::path::PathBuf {
        self.main.path().join(".synrepo/graph")
    }
}

fn worktree_fixture() -> WorktreeFixture {
    let main = tempdir().unwrap();
    let worktree = tempdir().unwrap();
    fs::create_dir_all(main.path().join("src")).unwrap();
    fs::write(main.path().join("src/lib.rs"), "pub fn main_root() {}\n").unwrap();

    git(main.path(), &["init"]);
    git(main.path(), &["config", "user.email", "test@example.com"]);
    git(main.path(), &["config", "user.name", "Test User"]);
    git(main.path(), &["add", "."]);
    git(main.path(), &["commit", "-m", "initial"]);
    fs::remove_dir(worktree.path()).unwrap();
    git(
        main.path(),
        &[
            "worktree",
            "add",
            "-b",
            "wt-branch",
            path_str(worktree.path()),
        ],
    );
    fs::write(
        worktree.path().join("src/wt_only.rs"),
        "pub fn wt_only() {}\n",
    )
    .unwrap();

    WorktreeFixture { main, worktree }
}

struct SubmoduleFixture {
    main: tempfile::TempDir,
    _sub_repo: tempfile::TempDir,
}

fn submodule_fixture() -> SubmoduleFixture {
    let sub_repo = tempdir().unwrap();
    fs::create_dir_all(sub_repo.path().join("src")).unwrap();
    fs::write(
        sub_repo.path().join("src/sub.rs"),
        "pub fn submodule() {}\n",
    )
    .unwrap();
    git(sub_repo.path(), &["init"]);
    git(
        sub_repo.path(),
        &["config", "user.email", "test@example.com"],
    );
    git(sub_repo.path(), &["config", "user.name", "Test User"]);
    git(sub_repo.path(), &["add", "."]);
    git(sub_repo.path(), &["commit", "-m", "sub initial"]);

    let main = tempdir().unwrap();
    fs::create_dir_all(main.path().join("src")).unwrap();
    fs::write(main.path().join("src/lib.rs"), "pub fn main_root() {}\n").unwrap();
    git(main.path(), &["init"]);
    git(main.path(), &["config", "user.email", "test@example.com"]);
    git(main.path(), &["config", "user.name", "Test User"]);
    git(main.path(), &["add", "."]);
    git(main.path(), &["commit", "-m", "main initial"]);
    git(
        main.path(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            path_str(sub_repo.path()),
            "vendor/sub",
        ],
    );

    SubmoduleFixture {
        main,
        _sub_repo: sub_repo,
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn path_str(path: &Path) -> &str {
    path.to_str().unwrap()
}
