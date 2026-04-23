use std::fs;
use std::ffi::OsString;
use tempfile::tempdir;

use crate::cli_support::agent_shims::{AgentTool, AutomationTier};
use crate::cli_support::commands::{
    step_apply_integration, step_apply_synthesis, step_ensure_ready, step_init, step_register_mcp,
    step_write_shim, StepOutcome,
};
use synrepo::tui::wizard::setup::synthesis::{CloudProvider, LocalPreset};
use synrepo::tui::{CloudCredentialSource, SynthesisChoice};
use toml::Value;

const TEST_GLOBAL_CONFIG_PATH_ENV: &str = "SYNREPO_TEST_GLOBAL_CONFIG_PATH";

struct GlobalConfigPathGuard {
    original: Option<OsString>,
}

impl GlobalConfigPathGuard {
    fn new(path: &std::path::Path) -> Self {
        let original = std::env::var_os(TEST_GLOBAL_CONFIG_PATH_ENV);
        std::env::set_var(TEST_GLOBAL_CONFIG_PATH_ENV, path);
        Self { original }
    }
}

impl Drop for GlobalConfigPathGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(TEST_GLOBAL_CONFIG_PATH_ENV, value),
            None => std::env::remove_var(TEST_GLOBAL_CONFIG_PATH_ENV),
        }
    }
}

#[test]
fn step_init_runs_init_on_empty_repo() {
    let dir = tempdir().unwrap();
    let outcome = step_init(dir.path(), None, false, false).unwrap();
    assert_eq!(outcome, StepOutcome::Applied);
    assert!(dir.path().join(".synrepo/config.toml").exists());
}

#[test]
fn step_init_skips_when_already_initialized() {
    let dir = tempdir().unwrap();
    step_init(dir.path(), None, false, false).unwrap();

    let again = step_init(dir.path(), None, false, false).unwrap();
    assert_eq!(again, StepOutcome::AlreadyCurrent);
}

#[test]
fn step_apply_synthesis_cloud_with_env_key_writes_only_repo_local_config() {
    let home = tempdir().unwrap();
    let _home_lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let _global_path_guard =
        GlobalConfigPathGuard::new(&home.path().join(".synrepo/config.toml"));
    let repo = tempdir().unwrap();
    step_init(repo.path(), None, false, false).unwrap();

    let choice = SynthesisChoice::Cloud {
        provider: CloudProvider::Anthropic,
        credential_source: CloudCredentialSource::Env,
        api_key: None,
    };

    let outcome = step_apply_synthesis(repo.path(), Some(&choice)).unwrap();
    assert_eq!(outcome, StepOutcome::Applied);

    let local = fs::read_to_string(repo.path().join(".synrepo/config.toml")).unwrap();
    assert!(local.contains("enabled = true"));
    assert!(local.contains("provider = \"anthropic\""));
    assert!(!local.contains("anthropic_api_key"));

    let global_path = home.path().join(".synrepo/config.toml");
    assert!(
        !global_path.exists(),
        "env-backed setup must not create a global config file"
    );
}

#[test]
fn step_apply_synthesis_cloud_with_new_key_saves_global_key_only() {
    let home = tempdir().unwrap();
    let _home_lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let _global_path_guard =
        GlobalConfigPathGuard::new(&home.path().join(".synrepo/config.toml"));
    fs::create_dir_all(home.path().join(".synrepo")).unwrap();
    fs::write(
        home.path().join(".synrepo/config.toml"),
        "[synthesis]\nlocal_preset = \"ollama\"\n",
    )
    .unwrap();

    let repo = tempdir().unwrap();
    step_init(repo.path(), None, false, false).unwrap();
    fs::write(
        repo.path().join(".synrepo/config.toml"),
        "[synthesis]\nenabled = false\nopenai_api_key = \"should-not-stay-local\"\n",
    )
    .unwrap();

    let choice = SynthesisChoice::Cloud {
        provider: CloudProvider::OpenAi,
        credential_source: CloudCredentialSource::EnteredGlobal,
        api_key: Some("sk-entered-openai".to_string()),
    };

    let outcome = step_apply_synthesis(repo.path(), Some(&choice)).unwrap();
    assert_eq!(outcome, StepOutcome::Applied);

    let local = fs::read_to_string(repo.path().join(".synrepo/config.toml")).unwrap();
    assert!(local.contains("provider = \"openai\""));
    assert!(!local.contains("openai_api_key"));

    let global_path = home.path().join(".synrepo/config.toml");
    let global: Value = fs::read_to_string(&global_path)
        .unwrap()
        .parse()
        .unwrap();
    let synthesis = global
        .get("synthesis")
        .and_then(Value::as_table)
        .expect("global config should keep a [synthesis] table");
    assert_eq!(
        synthesis.get("openai_api_key").and_then(Value::as_str),
        Some("sk-entered-openai")
    );
    assert_eq!(
        synthesis.get("local_preset").and_then(Value::as_str),
        Some("ollama")
    );
}

#[test]
fn step_apply_synthesis_local_saves_endpoint_in_global_config() {
    let home = tempdir().unwrap();
    let _home_lock =
        synrepo::test_support::global_test_lock(synrepo::config::test_home::HOME_ENV_TEST_LOCK);
    let _home_guard = synrepo::config::test_home::HomeEnvGuard::redirect_to(home.path());
    let _global_path_guard =
        GlobalConfigPathGuard::new(&home.path().join(".synrepo/config.toml"));
    let repo = tempdir().unwrap();
    step_init(repo.path(), None, false, false).unwrap();

    let choice = SynthesisChoice::Local {
        preset: LocalPreset::Custom,
        endpoint: "http://gpu-box:9000/v1/chat/completions".to_string(),
    };

    let outcome = step_apply_synthesis(repo.path(), Some(&choice)).unwrap();
    assert_eq!(outcome, StepOutcome::Applied);

    let local = fs::read_to_string(repo.path().join(".synrepo/config.toml")).unwrap();
    assert!(local.contains("provider = \"local\""));
    assert!(!local.contains("local_endpoint"));
    assert!(!local.contains("local_preset"));

    let global = fs::read_to_string(home.path().join(".synrepo/config.toml")).unwrap();
    assert!(global.contains("local_endpoint = \"http://gpu-box:9000/v1/chat/completions\""));
    assert!(global.contains("local_preset = \"custom\""));
}

#[test]
fn step_ensure_ready_runs_first_reconcile_when_state_is_missing() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("README.md"), "ready token\n").unwrap();
    step_init(dir.path(), None, false, false).unwrap();

    let outcome = step_ensure_ready(dir.path()).unwrap();

    assert_eq!(outcome, StepOutcome::Applied);
    assert!(dir
        .path()
        .join(".synrepo/state/reconcile-state.json")
        .exists());
}

#[test]
fn step_ensure_ready_skips_when_reconcile_state_exists() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("README.md"), "ready token\n").unwrap();
    step_init(dir.path(), None, false, false).unwrap();
    step_ensure_ready(dir.path()).unwrap();

    let outcome = step_ensure_ready(dir.path()).unwrap();

    assert_eq!(outcome, StepOutcome::AlreadyCurrent);
}

#[test]
fn step_register_mcp_claude_registers_then_idempotent() {
    let dir = tempdir().unwrap();
    let first = step_register_mcp(dir.path(), AgentTool::Claude).unwrap();
    assert_eq!(first, StepOutcome::Applied);
    assert!(dir.path().join(".mcp.json").exists());

    let second = step_register_mcp(dir.path(), AgentTool::Claude).unwrap();
    assert_eq!(second, StepOutcome::AlreadyCurrent);
}

#[test]
fn step_register_mcp_returns_not_automated_for_shim_only_targets() {
    let targets = [AgentTool::Copilot, AgentTool::Generic, AgentTool::Gemini];
    for target in targets {
        let dir = tempdir().unwrap();
        let outcome = step_register_mcp(dir.path(), target).unwrap();
        assert_eq!(
            outcome,
            StepOutcome::NotAutomated,
            "{target:?} must return NotAutomated"
        );
        assert!(
            !dir.path().join(".mcp.json").exists(),
            "{target:?} must not create .mcp.json"
        );
    }
}

#[test]
fn step_write_shim_writes_claude_shim() {
    let dir = tempdir().unwrap();
    let outcome = step_write_shim(dir.path(), AgentTool::Claude, false).unwrap();
    assert_eq!(outcome, StepOutcome::Applied);
    assert!(AgentTool::Claude.output_path(dir.path()).exists());
}

#[test]
fn step_apply_integration_writes_shim_and_registers_mcp() {
    let dir = tempdir().unwrap();
    let outcome = step_apply_integration(dir.path(), AgentTool::Claude, false).unwrap();
    assert_eq!(outcome, StepOutcome::Applied);
    assert!(AgentTool::Claude.output_path(dir.path()).exists());
    assert!(dir.path().join(".mcp.json").exists());
}

#[test]
fn step_apply_integration_rerun_is_idempotent() {
    let dir = tempdir().unwrap();
    step_apply_integration(dir.path(), AgentTool::Claude, false).unwrap();
    let mcp_first = fs::read(dir.path().join(".mcp.json")).unwrap();
    let shim_first = fs::read(AgentTool::Claude.output_path(dir.path())).unwrap();

    step_apply_integration(dir.path(), AgentTool::Claude, false).unwrap();

    let mcp_second = fs::read(dir.path().join(".mcp.json")).unwrap();
    let shim_second = fs::read(AgentTool::Claude.output_path(dir.path())).unwrap();
    assert_eq!(mcp_first, mcp_second, "MCP config must be byte-identical");
    assert_eq!(shim_first, shim_second, "shim must be byte-identical");
}

#[test]
fn step_apply_integration_for_shim_only_targets_still_writes_shim() {
    let targets = [
        AgentTool::Cursor,
        AgentTool::Copilot,
        AgentTool::Windsurf,
        AgentTool::Generic,
        AgentTool::Gemini,
    ];
    for target in targets {
        let dir = tempdir().unwrap();
        let outcome = step_apply_integration(dir.path(), target, false).unwrap();
        assert_eq!(
            outcome,
            StepOutcome::Applied,
            "{target:?} apply must surface shim Applied outcome"
        );
        assert!(
            target.output_path(dir.path()).exists(),
            "{target:?} shim file must be written"
        );
        assert!(
            !dir.path().join(".mcp.json").exists(),
            "{target:?} must not register MCP entry"
        );
    }
}

#[test]
fn automation_tier_matches_step_register_mcp_dispatch() {
    use clap::ValueEnum;

    for target in <AgentTool as ValueEnum>::value_variants() {
        let dir = tempdir().unwrap();
        let outcome = step_register_mcp(dir.path(), *target).unwrap();
        match target.automation_tier() {
            AutomationTier::Automated => assert!(
                matches!(
                    outcome,
                    StepOutcome::Applied | StepOutcome::AlreadyCurrent | StepOutcome::Updated
                ),
                "{target:?} is Automated but dispatch returned {outcome:?}"
            ),
            AutomationTier::ShimOnly => assert_eq!(
                outcome,
                StepOutcome::NotAutomated,
                "{target:?} is ShimOnly but dispatch returned {outcome:?}"
            ),
        }
    }
}

#[test]
fn step_init_surfaces_error_when_synrepo_path_is_blocked() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".synrepo"), "blocker").unwrap();
    let err = step_init(dir.path(), None, true, false);
    assert!(err.is_err(), "expected Err from blocked init, got {err:?}");
}
