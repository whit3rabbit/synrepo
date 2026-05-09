use anyhow::Context;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use synrepo::bootstrap::runtime_probe::{probe, RoutingDecision, RuntimeClassification};
use synrepo::config::{Mode, SemanticEmbeddingProvider, SemanticProviderSource};
use synrepo::tui::{
    run_embeddings_only_wizard, run_explain_only_wizard, run_setup_wizard, stdout_is_tty,
    DashboardOptions, EmbeddingSetupChoice, SetupPlan, SetupWizardOutcome, TuiOptions,
};

use super::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};
use super::apply_report::{show_apply_report_popup, ApplyReport, ApplyReportError};
use super::commands::{
    resolve_setup_scope, step_apply_explain, step_apply_integration, step_backup_mcp_config,
    step_ensure_ready, step_init_with_config,
};
use super::entry::{bare_ready_summary, bare_uninitialized_fallback};
use super::explain_cmd::print_explain_discovery_hint;
use super::repair_cmd::run_dashboard_with_sub_wizards;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitEntryMode {
    GuidedSetup,
    RawInit,
}

/// Dispatch explicit `synrepo init`.
///
/// Interactive no-flag init on a fresh repo is treated as an operator entry
/// into the full setup wizard. Flagged, scripted, ready, and partial cases
/// keep the raw bootstrap behavior.
pub(crate) fn run_init_or_setup(
    repo_root: &Path,
    mode: Option<Mode>,
    gitignore: bool,
    force: bool,
    opts: TuiOptions,
) -> anyhow::Result<()> {
    match init_entry_mode(repo_root, mode.is_some(), gitignore, force, stdout_is_tty()) {
        InitEntryMode::GuidedSetup => run_wizard_and_apply(repo_root, opts),
        InitEntryMode::RawInit => super::commands::init(repo_root, mode, gitignore, force),
    }
}

pub(crate) fn init_entry_mode(
    repo_root: &Path,
    has_mode_flag: bool,
    gitignore: bool,
    force: bool,
    is_tty: bool,
) -> InitEntryMode {
    let has_init_flags = has_mode_flag || gitignore || force;
    if !has_init_flags
        && is_tty
        && matches!(
            probe(repo_root).classification,
            RuntimeClassification::Uninitialized
        )
    {
        InitEntryMode::GuidedSetup
    } else {
        InitEntryMode::RawInit
    }
}

/// Run the TUI setup wizard and apply its [`SetupPlan`] outcome. Shared by the
/// bare-entrypoint OpenSetup arm and the explicit `synrepo setup` command when
/// invoked without a `<tool>` argument. Caller is responsible for the non-TTY
/// short-circuit before calling -- this helper still handles the wizard's own
/// `NonTty` outcome (printed by the wizard itself) defensively.
pub(crate) fn run_wizard_and_apply(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    match run_setup_wizard(repo_root, opts)? {
        SetupWizardOutcome::Completed { plan } => {
            match execute_setup_plan(repo_root, plan) {
                Ok(report) => show_apply_report_popup(opts, &report)?,
                Err(error) => {
                    show_apply_report_popup(opts, error.report())?;
                    return Err(error.into_anyhow());
                }
            }
            open_dashboard_after_wizard(repo_root, opts)
        }
        SetupWizardOutcome::Cancelled => {
            println!("setup wizard cancelled; no changes applied.");
            Ok(())
        }
        SetupWizardOutcome::NonTty => {
            eprint!("{}", bare_uninitialized_fallback());
            std::process::exit(2);
        }
    }
}

/// Execute a completed [`SetupPlan`] after the TUI alt-screen has been torn
/// down. All file-system writes happen here, not inside the library.
pub(crate) fn execute_setup_plan(
    repo_root: &Path,
    plan: SetupPlan,
) -> Result<ApplyReport, ApplyReportError> {
    let mut report = ApplyReport::new("setup complete");
    println!("synrepo setup: applying plan.");
    report.record_step("Runtime", || {
        step_init_with_config(repo_root, Some(plan.mode), false, false, |config| {
            seed_optional_setup_config(config, &plan);
        })
    })?;
    report.add_line(format!("Mode: {:?}", plan.mode));
    match plan.embedding_setup {
        EmbeddingSetupChoice::Disabled => report.add_line("Embeddings: disabled"),
        EmbeddingSetupChoice::Onnx => report.add_line("Embeddings: ONNX enabled"),
        EmbeddingSetupChoice::Ollama => report.add_line("Embeddings: Ollama enabled"),
    }
    if let Some(target) = plan.target {
        let tool = AgentTool::from_target_kind(target);
        let scope = resolve_setup_scope(repo_root, tool, false);
        let backup = if matches!(scope, agent_config::Scope::Local(_)) {
            report.record_value("MCP backup", || {
                step_backup_mcp_config(repo_root, tool, &scope)
            })?
        } else {
            None
        };
        report.add_backup(backup.as_deref());
        report.record_step("Agent integration", || {
            step_apply_integration(repo_root, tool, false, &scope)
        })?;
        let wrote_mcp = matches!(tool.automation_tier(), AutomationTier::Automated);
        shim_registry::record_install_best_effort(repo_root, tool, &scope, wrote_mcp, backup);
    } else {
        report.add_line("Agent integration: skipped");
    }
    if plan.explain.is_some() {
        report.record_step("Explain", || {
            step_apply_explain(repo_root, plan.explain.as_ref())
        })?;
        print_explain_discovery_hint();
    } else {
        report.add_line("Explain: skipped");
    }
    if plan.reconcile_after {
        // Setup promises an operationally ready repo, not just a populated
        // graph. The shared helper runs the first reconcile only when the
        // reconcile-state file is still missing.
        report.record_step("Ready check", || step_ensure_ready(repo_root))?;
    } else {
        report.add_line("Ready check: skipped");
    }
    report.record_value("Project registry", || {
        synrepo::registry::record_project(repo_root)
    })?;
    report.add_line("Project registry: recorded");
    println!("Setup complete. Repo is ready.");
    report.add_success("Setup complete. Repo is ready.");
    Ok(report)
}

fn seed_optional_setup_config(config: &mut synrepo::config::Config, plan: &SetupPlan) {
    apply_embedding_setup_to_config(config, plan.embedding_setup);
    if let Some(choice) = &plan.explain {
        config.explain.enabled = true;
        config.explain.provider = Some(
            match choice {
                synrepo::tui::ExplainChoice::Cloud { provider, .. } => provider.config_value(),
                synrepo::tui::ExplainChoice::Local { .. } => "local",
            }
            .to_string(),
        );
    }
}

pub(crate) fn apply_embedding_setup_to_config(
    config: &mut synrepo::config::Config,
    choice: EmbeddingSetupChoice,
) {
    config.enable_semantic_triage = choice.is_enabled();
    match choice {
        EmbeddingSetupChoice::Disabled => {}
        EmbeddingSetupChoice::Onnx => {
            config.semantic_embedding_provider = SemanticEmbeddingProvider::Onnx;
            config.semantic_embedding_provider_source = SemanticProviderSource::Explicit;
            config.semantic_model = "all-MiniLM-L6-v2".to_string();
            config.embedding_dim = 384;
        }
        EmbeddingSetupChoice::Ollama => {
            config.semantic_embedding_provider = SemanticEmbeddingProvider::Ollama;
            config.semantic_embedding_provider_source = SemanticProviderSource::Explicit;
            config.semantic_model = "all-minilm".to_string();
            config.embedding_dim = 384;
            config.semantic_ollama_endpoint = "http://localhost:11434".to_string();
            config.semantic_embedding_batch_size = 128;
        }
    }
}

pub(crate) fn apply_embedding_setup(
    repo_root: &Path,
    choice: EmbeddingSetupChoice,
) -> anyhow::Result<()> {
    if matches!(choice, EmbeddingSetupChoice::Disabled) {
        println!("  Embeddings left disabled; repo config unchanged.");
        return Ok(());
    }
    let local_path = repo_root.join(".synrepo").join("config.toml");
    let mut doc = load_toml_document(&local_path)?;
    doc.insert("enable_semantic_triage", Item::Value(TomlValue::from(true)));
    match choice {
        EmbeddingSetupChoice::Onnx => {
            doc.insert(
                "semantic_embedding_provider",
                Item::Value(TomlValue::from("onnx")),
            );
            doc.insert(
                "semantic_model",
                Item::Value(TomlValue::from("all-MiniLM-L6-v2")),
            );
            doc.insert("embedding_dim", Item::Value(TomlValue::from(384)));
        }
        EmbeddingSetupChoice::Ollama => {
            doc.insert(
                "semantic_embedding_provider",
                Item::Value(TomlValue::from("ollama")),
            );
            doc.insert("semantic_model", Item::Value(TomlValue::from("all-minilm")));
            doc.insert("embedding_dim", Item::Value(TomlValue::from(384)));
            doc.insert(
                "semantic_ollama_endpoint",
                Item::Value(TomlValue::from("http://localhost:11434")),
            );
            doc.insert(
                "semantic_embedding_batch_size",
                Item::Value(TomlValue::from(128)),
            );
        }
        EmbeddingSetupChoice::Disabled => unreachable!("disabled returned above"),
    }
    write_toml_document(&local_path, &doc)?;
    println!(
        "  Enabled {} embeddings in {}",
        match choice {
            EmbeddingSetupChoice::Onnx => "ONNX",
            EmbeddingSetupChoice::Ollama => "Ollama",
            EmbeddingSetupChoice::Disabled => unreachable!("disabled returned above"),
        },
        local_path.display()
    );
    Ok(())
}

/// Launch the explain-only sub-wizard after `synrepo setup <tool> --explain`,
/// patching repo-local `.synrepo/config.toml` plus user-scoped
/// `~/.synrepo/config.toml` as needed. Non-TTY callers get a pointer to the
/// relevant config files instead of crashing.
pub(crate) fn run_explain_step(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    match run_explain_only_wizard(opts)? {
        SetupWizardOutcome::Completed { plan } => {
            step_apply_explain(repo_root, plan.explain.as_ref())?;
            print_explain_discovery_hint();
            Ok(())
        }
        SetupWizardOutcome::Cancelled => {
            println!("explain sub-wizard cancelled; repo and user config untouched.");
            Ok(())
        }
        SetupWizardOutcome::NonTty => {
            println!(
                "--explain requires a TTY. Edit .synrepo/config.toml for repo-local \
                 enablement and ~/.synrepo/config.toml for reusable keys or local endpoints; \
                 see AGENTS.md for the `[explain]` block schema."
            );
            Ok(())
        }
    }
}

pub(crate) fn run_embeddings_setup_step(repo_root: &Path, opts: TuiOptions) -> anyhow::Result<()> {
    match run_embeddings_only_wizard(opts)? {
        SetupWizardOutcome::Completed { plan } => {
            apply_embedding_setup(repo_root, plan.embedding_setup)
        }
        SetupWizardOutcome::Cancelled => {
            println!("embeddings setup cancelled; repo config untouched.");
            Ok(())
        }
        SetupWizardOutcome::NonTty => {
            println!(
                "embeddings setup requires a TTY. Edit .synrepo/config.toml and set \
                 enable_semantic_triage plus semantic_embedding_provider to `onnx` or `ollama`."
            );
            Ok(())
        }
    }
}

fn load_toml_document(path: &Path) -> anyhow::Result<DocumentMut> {
    let raw = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };
    raw.parse().map_err(|err| {
        anyhow::anyhow!(
            "refusing to overwrite {}: file exists but is not valid TOML ({err})",
            path.display()
        )
    })
}

fn write_toml_document(path: &Path, doc: &DocumentMut) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    synrepo::util::atomic_write(path, doc.to_string().as_bytes())
        .with_context(|| format!("failed to atomically write {}", path.display()))
}

/// After a successful setup wizard, re-probe and open the dashboard with the
/// one-shot welcome banner seeded in the log pane. A partial re-classification
/// is unexpected here (setup just ran to completion), but we still fall
/// through gracefully rather than re-entering a wizard.
pub(crate) fn open_dashboard_after_wizard(
    repo_root: &Path,
    opts: TuiOptions,
) -> anyhow::Result<()> {
    if !stdout_is_tty() {
        return Ok(());
    }
    let report = probe(repo_root);
    let decision = RoutingDecision::from_report(&report);
    match decision {
        RoutingDecision::OpenDashboard { integration } => {
            let dashboard_opts = DashboardOptions {
                no_color: opts.no_color,
                welcome_banner: true,
            };
            run_dashboard_with_sub_wizards(repo_root, integration, dashboard_opts)
        }
        _ => {
            // Setup completed but probe still sees the repo as non-ready
            // (unusual — e.g. a compat-advisory left the store in a blocked
            // state). Surface the status summary when possible and fail
            // honestly so scripts do not treat the repo as operational.
            match bare_ready_summary(repo_root) {
                Ok(summary) => print!("{summary}"),
                Err(err) => eprintln!(
                    "Setup completed but the repo is not yet operational, and the status summary failed: {err:#}"
                ),
            }
            std::process::exit(2);
        }
    }
}
