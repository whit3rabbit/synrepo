use std::path::Path;

use agent_config::Scope;
use synrepo::config::{Config, SemanticEmbeddingProvider, SemanticProviderSource};
use synrepo::tui::{EmbeddingSetupChoice, SetupFlow, SetupPlan};

use super::agent_shims::{registry as shim_registry, AgentTool, AutomationTier};
use super::apply_report::{ApplyReport, ApplyReportError};
use super::commands::{
    step_add_root_gitignore, step_apply_explain, step_backup_mcp_config, step_ensure_ready,
    step_init_with_config, step_install_agent_hooks, step_register_mcp, step_write_shim,
};
use super::explain_cmd::print_explain_discovery_hint;

pub(crate) fn execute_setup_plan(
    repo_root: &Path,
    plan: SetupPlan,
) -> Result<ApplyReport, ApplyReportError> {
    let mut report = ApplyReport::new("setup complete");
    println!("synrepo setup: applying plan.");
    if plan.flow == SetupFlow::Full {
        report.record_step("Runtime", || {
            step_init_with_config(repo_root, Some(plan.mode), false, false, |config| {
                seed_optional_setup_config(config, &plan);
            })
        })?;
        report.add_line(format!("Mode: {:?}", plan.mode));
    } else {
        report.add_line("Runtime: already initialized");
    }
    if plan.add_root_gitignore {
        report.record_step("Root gitignore", || step_add_root_gitignore(repo_root))?;
    } else {
        report.add_line("Root gitignore: unchanged");
    }
    apply_setup_integration(repo_root, &plan, &mut report)?;
    if plan.flow == SetupFlow::Full {
        apply_optional_provider_setup(repo_root, &plan, &mut report)?;
    }
    report.record_value("Project registry", || {
        synrepo::registry::record_project(repo_root)
    })?;
    report.add_line("Project registry: recorded");
    println!("Setup complete. Repo is ready.");
    report.add_success("Setup complete. Repo is ready.");
    Ok(report)
}

fn apply_setup_integration(
    repo_root: &Path,
    plan: &SetupPlan,
    report: &mut ApplyReport,
) -> Result<(), ApplyReportError> {
    let Some(target) = plan.target else {
        report.add_line("Agent integration: skipped");
        return Ok(());
    };
    let tool = AgentTool::from_target_kind(target);
    let scope = Scope::Local(repo_root.to_path_buf());
    let mut backup = None;
    if plan.write_agent_shim {
        report.record_step("Shim", || step_write_shim(repo_root, tool, &scope, false))?;
    } else if plan.register_mcp {
        report.add_line("Shim: deferred until MCP registration");
    } else {
        report.add_line("Shim: unchanged");
    }
    if plan.register_mcp {
        if !plan.write_agent_shim {
            report.record_step("Shim", || step_write_shim(repo_root, tool, &scope, false))?;
        }
        if matches!(scope, Scope::Local(_)) {
            backup = report.record_value("MCP backup", || {
                step_backup_mcp_config(repo_root, tool, &scope)
            })?;
            report.add_backup(backup.as_deref());
        }
        report.record_step("MCP", || step_register_mcp(repo_root, tool, &scope))?;
    } else {
        report.add_line("MCP: skipped");
    }
    if plan.install_agent_hooks {
        report.record_step("Hooks", || step_install_agent_hooks(repo_root, tool))?;
    } else {
        report.add_line("Hooks: unchanged");
    }
    if plan.write_agent_shim || plan.register_mcp {
        let wrote_mcp =
            plan.register_mcp && matches!(tool.automation_tier(), AutomationTier::Automated);
        shim_registry::record_install_best_effort(repo_root, tool, &scope, wrote_mcp, backup);
    }
    Ok(())
}

fn apply_optional_provider_setup(
    repo_root: &Path,
    plan: &SetupPlan,
    report: &mut ApplyReport,
) -> Result<(), ApplyReportError> {
    match plan.embedding_setup {
        EmbeddingSetupChoice::Disabled => report.add_line("Embeddings: disabled"),
        EmbeddingSetupChoice::Onnx => report.add_line("Embeddings: ONNX enabled"),
        EmbeddingSetupChoice::Ollama => report.add_line("Embeddings: Ollama enabled"),
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
        report.record_step("Ready check", || step_ensure_ready(repo_root))?;
    } else {
        report.add_line("Ready check: skipped");
    }
    Ok(())
}

fn seed_optional_setup_config(config: &mut Config, plan: &SetupPlan) {
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

fn apply_embedding_setup_to_config(config: &mut Config, choice: EmbeddingSetupChoice) {
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
