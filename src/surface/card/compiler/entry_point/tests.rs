use super::*;
use crate::surface::card::compiler::test_support::bootstrap;
use crate::surface::card::compiler::{CardCompiler, GraphCardCompiler};
use std::fs;
use tempfile::tempdir;

// 7.1: one test per detection rule — match and non-match cases

#[test]
fn binary_rule_matches_main_in_src_main() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/main.rs"), "fn main() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    let kinds: Vec<EntryPointKind> = card.entry_points.iter().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&EntryPointKind::Binary),
        "expected Binary in {kinds:?}"
    );
    let binary = card
        .entry_points
        .iter()
        .find(|e| e.kind == EntryPointKind::Binary)
        .unwrap();
    assert_eq!(binary.qualified_name, "main");
}

#[test]
fn binary_rule_matches_flutter_main_in_lib_main_dart() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("lib")).unwrap();
    fs::write(repo.path().join("pubspec.yaml"), "name: app\n").unwrap();
    fs::write(repo.path().join("lib/main.dart"), "void main() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    let binary = card
        .entry_points
        .iter()
        .find(|e| e.kind == EntryPointKind::Binary)
        .expect("expected Flutter lib/main.dart entrypoint");
    assert_eq!(binary.qualified_name, "main");
    assert!(
        binary.location.starts_with("lib/main.dart:"),
        "unexpected binary location: {}",
        binary.location
    );
}

#[test]
fn binary_rule_matches_dart_bin_main() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("bin")).unwrap();
    fs::write(repo.path().join("pubspec.yaml"), "name: tool\n").unwrap();
    fs::write(repo.path().join("bin/tool.dart"), "void main() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .any(|e| e.kind == EntryPointKind::Binary && e.location.starts_with("bin/tool.dart:")),
        "expected Dart bin/tool.dart entrypoint in {:?}",
        card.entry_points
    );
}

#[test]
fn binary_rule_does_not_match_main_in_lib_rs() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    // main in lib.rs is NOT a binary entry point
    fs::write(repo.path().join("src/lib.rs"), "fn main() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .all(|e| e.kind != EntryPointKind::Binary),
        "lib.rs main must not be Binary"
    );
}

#[test]
fn cli_command_rule_matches_function_in_cli_path() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/cli")).unwrap();
    fs::write(repo.path().join("src/cli/mod.rs"), "pub fn run() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .any(|e| e.kind == EntryPointKind::CliCommand),
        "expected CliCommand in {:?}",
        card.entry_points.iter().map(|e| e.kind).collect::<Vec<_>>()
    );
}

#[test]
fn cli_command_rule_does_not_match_non_cli_path() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    // A function named `run` in a non-cli file should not be CliCommand.
    fs::write(repo.path().join("src/service.rs"), "pub fn run() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .all(|e| e.kind != EntryPointKind::CliCommand),
        "service.rs run() must not be CliCommand"
    );
}

#[test]
fn http_handler_rule_matches_handle_prefix() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(
        repo.path().join("src/server.rs"),
        "fn handle_request() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .any(|e| e.kind == EntryPointKind::HttpHandler),
        "expected HttpHandler in {:?}",
        card.entry_points.iter().map(|e| e.kind).collect::<Vec<_>>()
    );
}

#[test]
fn http_handler_rule_does_not_match_plain_function() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/server.rs"), "fn process() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .all(|e| e.kind != EntryPointKind::HttpHandler),
        "process() must not be HttpHandler"
    );
}

#[test]
fn http_handler_rule_does_not_match_struct_in_router_path() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/router")).unwrap();
    // A struct in a router path should NOT be HttpHandler.
    fs::write(
        repo.path().join("src/router/utils.rs"),
        "pub struct Config {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .all(|e| e.kind != EntryPointKind::HttpHandler),
        "struct in router path must not be HttpHandler"
    );
}

#[test]
fn lib_root_rule_matches_function_in_lib_rs() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    fs::write(repo.path().join("src/lib.rs"), "pub fn init() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .any(|e| e.kind == EntryPointKind::LibRoot),
        "expected LibRoot in {:?}",
        card.entry_points.iter().map(|e| e.kind).collect::<Vec<_>>()
    );
}

#[test]
fn lib_root_rule_does_not_match_non_module_root() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src")).unwrap();
    // init() in a regular file should NOT be LibRoot.
    fs::write(repo.path().join("src/service.rs"), "pub fn init() {}\n").unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    assert!(
        card.entry_points
            .iter()
            .all(|e| e.kind != EntryPointKind::LibRoot),
        "service.rs init() must not be LibRoot"
    );
}

// 7.2: rule ordering — first matching rule wins

#[test]
fn rule_ordering_cli_path_beats_handle_prefix() {
    let repo = tempdir().unwrap();
    // handle_command in src/cli/handler.rs matches CliCommand (rule 2) before
    // HttpHandler (rule 3) because `cli` appears in the path.
    fs::create_dir_all(repo.path().join("src/cli")).unwrap();
    fs::write(
        repo.path().join("src/cli/handler.rs"),
        "pub fn handle_command() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    let entry = card
        .entry_points
        .iter()
        .find(|e| e.qualified_name == "handle_command");
    assert!(entry.is_some(), "handle_command should be detected");
    assert_eq!(
        entry.unwrap().kind,
        EntryPointKind::CliCommand,
        "cli path segment must take priority over handle_ prefix"
    );
}

#[test]
fn rule_ordering_only_one_entry_per_symbol() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join("src/cli")).unwrap();
    fs::write(
        repo.path().join("src/cli/handler.rs"),
        "pub fn handle_command() {}\n",
    )
    .unwrap();

    let graph = bootstrap(&repo);
    let compiler = GraphCardCompiler::new(Box::new(graph), Some(repo.path()));
    let card = compiler.entry_point_card(None, Budget::Tiny).unwrap();

    let count = card
        .entry_points
        .iter()
        .filter(|e| e.qualified_name == "handle_command")
        .count();
    assert_eq!(count, 1, "handle_command must appear exactly once");
}
