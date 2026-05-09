use std::fs;

use tempfile::tempdir;

use crate::cli_support::setup_cmd::{execute_setup_plan, init_entry_mode, InitEntryMode};
use crate::cli_support::tests::support::canonicalize_no_verbatim;
use synrepo::bootstrap::runtime_probe::AgentTargetKind;
use synrepo::config::{Config, Mode};
use synrepo::tui::{EmbeddingSetupChoice, SetupFlow, SetupPlan};

fn redirect_home() -> (
    synrepo::test_support::GlobalTestLock,
    tempfile::TempDir,
    synrepo::config::test_home::HomeEnvGuard,
) {
    let lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let home = tempdir().unwrap();
    let canonical_home = canonicalize_no_verbatim(home.path());
    let guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(&canonical_home);
    (lock, home, guard)
}

fn full_setup_plan() -> SetupPlan {
    SetupPlan {
        flow: SetupFlow::Full,
        mode: Mode::Auto,
        target: None,
        add_root_gitignore: true,
        write_agent_shim: false,
        register_mcp: false,
        install_agent_hooks: false,
        embedding_setup: EmbeddingSetupChoice::Disabled,
        explain: None,
        reconcile_after: true,
    }
}

#[test]
fn uninitialized_tty_no_flag_init_routes_to_guided_setup() {
    let repo = tempdir().unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::GuidedSetup);
}

#[test]
fn non_tty_init_stays_raw() {
    let repo = tempdir().unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, false);

    assert_eq!(mode, InitEntryMode::RawInit);
}

#[test]
fn flagged_init_stays_raw_even_in_tty() {
    let repo = tempdir().unwrap();

    let cases = [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ];

    for (has_mode_flag, gitignore, force) in cases {
        let mode = init_entry_mode(repo.path(), has_mode_flag, gitignore, force, true);
        assert_eq!(
            mode,
            InitEntryMode::RawInit,
            "flags must keep init scriptable: {has_mode_flag:?} {gitignore:?} {force:?}"
        );
    }
}

#[test]
fn ready_repo_with_missing_followups_routes_to_guided_setup() {
    let repo = tempdir().unwrap();
    synrepo::bootstrap::bootstrap(repo.path(), None, false).unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::GuidedSetup);
}

#[test]
fn fully_current_ready_repo_init_stays_raw() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "ready current\n").unwrap();
    let mut plan = full_setup_plan();
    plan.target = Some(AgentTargetKind::Codex);
    plan.write_agent_shim = true;
    plan.register_mcp = true;
    plan.install_agent_hooks = true;
    execute_setup_plan(repo.path(), plan).unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::RawInit);
}

#[test]
fn followup_plan_execution_writes_project_integration_hooks_and_gitignore() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "followup execution\n").unwrap();
    synrepo::bootstrap::bootstrap(repo.path(), None, false).unwrap();
    let mut plan = full_setup_plan();
    plan.flow = SetupFlow::FollowUp;
    plan.target = Some(AgentTargetKind::Codex);
    plan.write_agent_shim = true;
    plan.register_mcp = true;
    plan.install_agent_hooks = true;
    plan.reconcile_after = false;

    let report = execute_setup_plan(repo.path(), plan).unwrap();

    assert!(report
        .lines()
        .contains(&"Runtime: already initialized".to_string()));
    assert!(repo.path().join(".agents/skills/synrepo/SKILL.md").exists());
    assert!(fs::read_to_string(repo.path().join(".codex/config.toml"))
        .unwrap()
        .contains("[mcp_servers.synrepo]"));
    assert!(fs::read_to_string(repo.path().join(".codex/hooks.json"))
        .unwrap()
        .contains("synrepo agent-hook nudge --client codex"));
    assert!(fs::read_to_string(repo.path().join(".gitignore"))
        .unwrap()
        .lines()
        .any(|line| line.trim() == ".synrepo/"));
}

#[test]
fn ready_repo_missing_supported_hooks_routes_to_guided_setup() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "ready hooks\n").unwrap();
    let mut plan = full_setup_plan();
    plan.target = Some(AgentTargetKind::Codex);
    plan.write_agent_shim = true;
    plan.register_mcp = true;
    execute_setup_plan(repo.path(), plan).unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::GuidedSetup);
}

#[test]
fn partial_repo_init_stays_raw() {
    let repo = tempdir().unwrap();
    fs::create_dir_all(repo.path().join(".synrepo")).unwrap();

    let mode = init_entry_mode(repo.path(), false, false, false, true);

    assert_eq!(mode, InitEntryMode::RawInit);
}

#[test]
fn wizard_plan_execution_registers_project() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "setup registry test\n").unwrap();

    let report = execute_setup_plan(repo.path(), full_setup_plan()).unwrap();

    assert!(report.lines().contains(&"Runtime: applied".to_string()));
    assert!(report
        .lines()
        .contains(&"Agent integration: skipped".to_string()));
    assert!(report
        .lines()
        .contains(&"Project registry: recorded".to_string()));
    assert!(report
        .lines()
        .contains(&"Status: Setup complete. Repo is ready.".to_string()));
    assert!(synrepo::registry::get(repo.path()).unwrap().is_some());
}

#[test]
fn wizard_plan_execution_persists_embeddings_opt_in() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(repo.path().join("README.md"), "setup embeddings test\n").unwrap();

    execute_setup_plan(
        repo.path(),
        SetupPlan {
            embedding_setup: EmbeddingSetupChoice::Onnx,
            ..full_setup_plan()
        },
    )
    .unwrap();

    let config = Config::load(repo.path()).unwrap();
    assert!(config.enable_semantic_triage);
    assert_eq!(
        config.semantic_embedding_provider,
        synrepo::config::SemanticEmbeddingProvider::Onnx
    );
    assert_eq!(config.semantic_model, "all-MiniLM-L6-v2");
    assert!(fs::read_to_string(repo.path().join(".synrepo/config.toml"))
        .unwrap()
        .contains("enable_semantic_triage = true"));
    assert!(fs::read_to_string(repo.path().join(".synrepo/config.toml"))
        .unwrap()
        .contains("semantic_embedding_provider = \"onnx\""));
}

#[test]
fn wizard_plan_execution_persists_ollama_embeddings_opt_in() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("README.md"),
        "setup ollama embeddings test\n",
    )
    .unwrap();

    execute_setup_plan(
        repo.path(),
        SetupPlan {
            embedding_setup: EmbeddingSetupChoice::Ollama,
            ..full_setup_plan()
        },
    )
    .unwrap();

    let config = Config::load(repo.path()).unwrap();
    assert!(config.enable_semantic_triage);
    assert_eq!(
        config.semantic_embedding_provider,
        synrepo::config::SemanticEmbeddingProvider::Ollama
    );
    assert_eq!(config.semantic_model, "all-minilm");
    assert_eq!(config.embedding_dim, 384);
    assert_eq!(config.semantic_ollama_endpoint, "http://localhost:11434");
    assert_eq!(config.semantic_embedding_batch_size, 128);
}

#[test]
#[cfg(feature = "semantic-triage")]
fn setup_init_with_semantic_triage_does_not_build_vectors_index() {
    let (_lock, _home, _guard) = redirect_home();
    let repo = tempdir().unwrap();
    fs::write(
        repo.path().join("README.md"),
        "# Test Concept\n\nA setup concept for embedding.\n",
    )
    .unwrap();

    execute_setup_plan(
        repo.path(),
        SetupPlan {
            embedding_setup: EmbeddingSetupChoice::Onnx,
            ..full_setup_plan()
        },
    )
    .unwrap();

    let vectors_dir = repo.path().join(".synrepo/index/vectors");
    assert!(
        !vectors_dir.join("index.bin").exists(),
        "setup must not build embeddings implicitly"
    );
}
