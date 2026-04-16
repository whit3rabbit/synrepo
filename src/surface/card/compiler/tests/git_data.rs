//! git-data-surfacing-v1 tests: FileCard.git_intelligence and
//! SymbolCard.last_change population across budget tiers and config modes.

use super::super::{Budget, GraphCardCompiler};
use super::fixtures::git_backed_fixture;
use crate::{
    config::Config,
    pipeline::git::test_support::{git, init_commit},
    structure::graph::GraphStore,
    surface::card::{CardCompiler, LastChangeGranularity},
};
use std::fs;
use tempfile::tempdir;

#[test]
fn file_card_normal_populates_git_intelligence_when_repo_has_history() {
    let (_repo, compiler, file_id, _sym_id) = git_backed_fixture();
    let card = compiler.file_card(file_id, Budget::Normal).unwrap();
    let gi = card
        .git_intelligence
        .expect("git_intelligence must be populated at Normal budget in a git repo");
    assert!(!gi.commits.is_empty(), "must record at least one commit");
    let owner = gi.ownership.expect("ownership must be present");
    assert_eq!(owner.primary_author, "synrepo");
}

#[test]
fn file_card_tiny_omits_git_intelligence() {
    let (_repo, compiler, file_id, _sym_id) = git_backed_fixture();
    let card = compiler.file_card(file_id, Budget::Tiny).unwrap();
    assert!(
        card.git_intelligence.is_none(),
        "Tiny budget must not include git_intelligence"
    );
}

#[test]
fn file_card_git_intelligence_is_none_without_config() {
    // Same fixture but skip `.with_config`: the compiler must gracefully
    // degrade rather than erroring.
    let repo = tempdir().unwrap();
    init_commit(&repo);
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    git(&repo, &["add", "src/lib.rs"]);
    git(&repo, &["commit", "-m", "add foo"]);
    let graph = super::super::test_support::bootstrap(&repo);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.file_card(file_id, Budget::Normal).unwrap();
    assert!(card.git_intelligence.is_none());
}

#[test]
fn file_card_git_intelligence_is_none_without_git() {
    // No `git init`: the resolver must swallow the missing-repo condition
    // and return None rather than error.
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn foo() {}\n").unwrap();
    let graph = super::super::test_support::bootstrap(&repo);
    let file_id = graph.file_by_path("src/lib.rs").unwrap().unwrap().id;
    let compiler =
        GraphCardCompiler::new(Box::new(graph), Some(repo.path())).with_config(Config::default());
    let card = compiler.file_card(file_id, Budget::Normal).unwrap();
    assert!(
        card.git_intelligence.is_none(),
        "non-git repo must yield None, not error"
    );
}

#[test]
fn symbol_card_normal_last_change_uses_file_granularity_without_summary() {
    let (_repo, compiler, _file_id, sym_id) = git_backed_fixture();
    let card = compiler.symbol_card(sym_id, Budget::Normal).unwrap();
    let lc = card.last_change.expect("last_change must be populated");
    assert_eq!(lc.granularity, LastChangeGranularity::File);
    assert_eq!(lc.summary, None, "Normal budget must omit summary");
    assert_eq!(
        lc.revision.len(),
        12,
        "revision must be shortened to 12 hex chars"
    );
    assert_eq!(lc.author_name, "synrepo");
}

#[test]
fn symbol_card_deep_last_change_includes_summary() {
    let (_repo, compiler, _file_id, sym_id) = git_backed_fixture();
    let card = compiler.symbol_card(sym_id, Budget::Deep).unwrap();
    let lc = card.last_change.expect("last_change must be populated");
    assert_eq!(lc.granularity, LastChangeGranularity::File);
    assert_eq!(
        lc.summary.as_deref(),
        Some("add add"),
        "Deep budget must include commit summary"
    );
}

#[test]
fn symbol_card_tiny_has_no_last_change() {
    let (_repo, compiler, _file_id, sym_id) = git_backed_fixture();
    let card = compiler.symbol_card(sym_id, Budget::Tiny).unwrap();
    assert!(card.last_change.is_none());
}
