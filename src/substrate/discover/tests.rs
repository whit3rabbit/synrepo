use super::*;
use crate::config::Config;
use std::fs;
use tempfile::tempdir;

#[test]
fn discover_respects_roots_gitignore_and_redaction() {
    let repo = tempdir().unwrap();
    fs::write(repo.path().join(".gitignore"), "src/ignored.rs\n").unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::create_dir_all(repo.path().join("docs")).unwrap();

    fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
    fs::write(repo.path().join("src/ignored.rs"), "pub fn ignored() {}\n").unwrap();
    fs::write(repo.path().join("docs/guide.md"), "# guide\n").unwrap();
    fs::write(repo.path().join("docs/app.env"), "SECRET=1\n").unwrap();
    fs::write(repo.path().join("top.txt"), "outside configured roots\n").unwrap();

    let config = Config {
        roots: vec!["src".to_string(), "docs".to_string()],
        ..Config::default()
    };

    let discovered = discover(repo.path(), &config).unwrap();
    let relative_paths: Vec<_> = discovered
        .into_iter()
        .map(|file| file.relative_path)
        .collect();

    assert_eq!(
        relative_paths,
        vec!["docs/guide.md".to_string(), "src/lib.rs".to_string()]
    );
}

#[test]
fn discover_never_walks_into_generated_runtime_state() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn real_code() {}\n").unwrap();

    fs::create_dir_all(repo.path().join(".synrepo/graph")).unwrap();
    fs::write(
        repo.path().join(".synrepo/graph/nodes.db"),
        "SQLite format 3\0",
    )
    .unwrap();
    fs::write(
        repo.path().join(".synrepo/config.toml"),
        "mode = \"auto\"\n",
    )
    .unwrap();
    fs::create_dir_all(repo.path().join(".syntext")).unwrap();
    fs::write(repo.path().join(".syntext/manifest.json"), "{}").unwrap();
    fs::write(
        repo.path().join(".syntext/segment.post"),
        "pub fn external_index_noise() {}\n",
    )
    .unwrap();

    let discovered = discover(repo.path(), &Config::default()).unwrap();
    let paths: Vec<_> = discovered
        .iter()
        .map(|f| f.relative_path.as_str())
        .collect();
    assert!(paths.iter().all(|p| !p.starts_with(".synrepo")));
    assert!(paths.iter().all(|p| !p.starts_with(".syntext")));
    assert!(paths.contains(&"src/lib.rs"));
}

#[test]
fn discover_skips_non_text_and_oversized_files() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();

    fs::write(repo.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
    fs::write(repo.path().join("src/blob.bin"), [0, 159, 146, 150]).unwrap();
    fs::write(repo.path().join("src/empty.txt"), "").unwrap();
    fs::write(
        repo.path().join("src/big.txt"),
        "abcdefghijklmnopqrstuvwxyz",
    )
    .unwrap();

    let config = Config {
        max_file_size_bytes: 20,
        ..Config::default()
    };

    let discovered = discover(repo.path(), &config).unwrap();
    let relative_paths: Vec<_> = discovered
        .into_iter()
        .map(|file| file.relative_path)
        .collect();

    assert_eq!(relative_paths, vec!["src/lib.rs".to_string()]);
}
