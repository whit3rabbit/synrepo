use std::{fs, path::Path};

use synrepo::config::{Config, Mode};
use synrepo::pipeline::explain::telemetry;
use synrepo::pipeline::maintenance::plan_maintenance;
use synrepo::pipeline::repair::{refresh_commentary, ActionContext};
use synrepo::pipeline::writer::{acquire_write_admission, map_lock_error};
use synrepo::surface::card::{Budget, CardCompiler};

use agent_config::{AgentConfigError, InstallReport, Scope};

mod local_shim;

use crate::cli_support::agent_shims::{
    registry as shim_registry, AgentTool, ShimPlacement, SYNREPO_INSTALL_OWNER,
};

use super::watch::ensure_watch_not_running;

/// Initialize the repository with the specified mode.
///
/// `force = true` recreates the runtime in place when the canonical graph
/// store is blocked by an incompatible storage snapshot, replacing the legacy
/// "rm -rf .synrepo/ && synrepo init" recipe. The watch and writer-lock gates
/// are still enforced; force never bypasses a live mutator.
pub(crate) fn init(
    repo_root: &Path,
    requested_mode: Option<Mode>,
    gitignore: bool,
    force: bool,
    generate_commentary: bool,
) -> anyhow::Result<()> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "init")?;

    let report =
        synrepo::bootstrap::bootstrap_with_force(repo_root, requested_mode, gitignore, force)?;
    print!("{}", report.render());
    if generate_commentary {
        generate_commentary_after_init(repo_root, &synrepo_dir)?;
    }
    Ok(())
}

fn generate_commentary_after_init(repo_root: &Path, synrepo_dir: &Path) -> anyhow::Result<()> {
    println!(
        "Generating commentary because --generate-commentary was set. \
         This may call the configured explain provider with repository context."
    );
    let config = Config::load(repo_root)?;
    let _writer_lock = acquire_write_admission(synrepo_dir, "init --generate-commentary")
        .map_err(|err| map_lock_error("init --generate-commentary", err))?;
    let maint_plan = plan_maintenance(synrepo_dir, &config);
    let context = ActionContext {
        repo_root,
        synrepo_dir,
        config: &config,
        maint_plan: &maint_plan,
    };
    let mut actions_taken = Vec::new();
    let result = telemetry::with_synrepo_dir(synrepo_dir, || {
        refresh_commentary(&context, &mut actions_taken, None, None, None)
    });
    synrepo::pipeline::context_metrics::record_commentary_refresh_best_effort(
        synrepo_dir,
        result.is_err(),
    );
    result?;
    if actions_taken.is_empty() {
        println!("  Commentary generation completed; no targets needed generation.");
    } else {
        for action in actions_taken {
            println!("  {action}");
        }
    }
    Ok(())
}

/// Render the change risk output as a String (test-friendly equivalent of `change_risk`).
/// Output is identical to what `change_risk` prints, including trailing newlines.
pub(crate) fn change_risk_output(
    repo_root: &Path,
    target: &str,
    budget: Option<&str>,
    json: bool,
) -> anyhow::Result<String> {
    use synrepo::store::sqlite::SqliteGraphStore;

    let synrepo_dir = Config::synrepo_dir(repo_root);
    ensure_watch_not_running(&synrepo_dir, "change-risk")?;

    let budget = match budget {
        Some("tiny") => Budget::Tiny,
        Some("normal") => Budget::Normal,
        Some("deep") => Budget::Deep,
        Some(b) => anyhow::bail!("invalid budget: {b} (expected tiny, normal, or deep)"),
        None => Budget::Tiny,
    };

    let graph_dir = synrepo_dir.join("graph");
    let graph = SqliteGraphStore::open_existing(&graph_dir)?;
    let config = Config::load(repo_root)?;

    let compiler =
        synrepo::surface::card::compiler::GraphCardCompiler::new(Box::new(graph), Some(repo_root))
            .with_config(config);

    let node_id = compiler
        .resolve_target(target)?
        .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;

    let card = compiler.change_risk_card(node_id, budget)?;

    let mut out = String::new();
    if json {
        out.push_str(&serde_json::to_string_pretty(&card)?);
    } else {
        out.push_str(&format!(
            "Change Risk: {} ({})\n",
            card.target_name, card.target_kind
        ));
        out.push_str(&format!("  Risk level: {:?}\n", card.risk_level));
        out.push_str(&format!("  Risk score: {:.2}\n", card.risk_score));
        if !card.risk_factors.is_empty() {
            out.push_str("  Factors:\n");
            for f in &card.risk_factors {
                out.push_str(&format!(
                    "    - {}: {:.2} ({})\n",
                    f.signal, f.normalized_value, f.description
                ));
            }
        }
    }
    out.push('\n');
    Ok(out)
}

/// Output change risk assessment for a target.
pub(crate) fn change_risk(
    repo_root: &Path,
    target: &str,
    budget: Option<&str>,
    json: bool,
) -> anyhow::Result<()> {
    let out = change_risk_output(repo_root, target, budget, json)?;
    print!("{}", out);
    Ok(())
}

/// Generate the agent skill or instructions file for the specified agent CLI.
pub(crate) fn agent_setup(
    repo_root: &Path,
    tool: AgentTool,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    agent_setup_with_scope(
        repo_root,
        tool,
        &Scope::Local(repo_root.to_path_buf()),
        force,
        regen,
    )
}

pub(crate) fn agent_setup_with_scope(
    repo_root: &Path,
    tool: AgentTool,
    scope: &Scope,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    match tool.placement_kind() {
        ShimPlacement::Skill { name } => install_skill_shim(tool, scope, name, force, regen)?,
        ShimPlacement::Instruction { name, placement } => {
            install_instruction_shim(tool, scope, name, placement, force, regen)?
        }
        ShimPlacement::Local => write_local_shim(repo_root, tool, force, regen)?,
    }
    println!("  {}", tool.include_instruction());

    // Shim-only: no MCP config written, so removal should not expect an
    // MCP entry for this agent in the project registry.
    shim_registry::record_install_best_effort(repo_root, tool, scope, false, None);

    Ok(())
}

fn write_local_shim(
    repo_root: &Path,
    tool: AgentTool,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    let out_path = tool.output_path(repo_root);
    let content = tool.shim_content();
    let label = tool.artifact_label();

    local_shim::reject_parent_symlinks(repo_root, &out_path)?;
    let target_meta = fs::symlink_metadata(&out_path)
        .map(Some)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            }
        })
        .map_err(|error| anyhow::anyhow!("could not inspect {}: {error}", out_path.display()))?;
    let target_is_symlink = target_meta
        .as_ref()
        .is_some_and(|meta| meta.file_type().is_symlink());

    if regen && target_meta.is_some() && !target_is_symlink {
        let existing = fs::read_to_string(&out_path).unwrap_or_default();
        if existing == content {
            println!(
                "{} {label} is already current: {}",
                tool.display_name(),
                out_path.display()
            );
            return Ok(());
        }
        // Different content: fall through to write.
        println!(
            "Updating {} {label} (content changed): {}",
            tool.display_name(),
            out_path.display()
        );
    } else if target_meta.is_some() && !force && !regen {
        println!(
            "synrepo agent-setup: {} already exists.",
            out_path.display()
        );
        println!("  Pass --force to overwrite, or --regen to update if stale.");
        return Ok(());
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| anyhow::anyhow!("could not create {}: {error}", parent.display()))?;
    }
    local_shim::reject_parent_symlinks(repo_root, &out_path)?;
    if target_is_symlink {
        fs::remove_file(&out_path).map_err(|error| {
            anyhow::anyhow!("could not replace symlink {}: {error}", out_path.display())
        })?;
    }

    local_shim::write_atomic(&out_path, content)
        .map_err(|error| anyhow::anyhow!("could not write {}: {error}", out_path.display()))?;

    if !regen {
        println!(
            "Wrote {} {label}: {}",
            tool.display_name(),
            out_path.display()
        );
    }
    Ok(())
}

fn install_skill_shim(
    tool: AgentTool,
    scope: &Scope,
    name: &str,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    let Some(id) = tool.agent_config_id() else {
        anyhow::bail!("{} has no agent-config integration id", tool.display_name());
    };
    let installer = agent_config::skill_by_id(id).ok_or_else(|| {
        anyhow::anyhow!(
            "{} does not support agent-config skills",
            tool.display_name()
        )
    })?;
    let spec = agent_config::SkillSpec::builder(name)
        .owner(SYNREPO_INSTALL_OWNER)
        .description(tool.skill_description())
        .body(tool.shim_spec_body())
        .adopt_unowned(force || regen)
        .try_build()?;
    let report = installer
        .install_skill(scope, &spec)
        .map_err(|err| installer_error(tool, "skill", err))?;
    print_agent_install_report(tool, "skill", &report);
    Ok(())
}

fn install_instruction_shim(
    tool: AgentTool,
    scope: &Scope,
    name: &str,
    placement: agent_config::InstructionPlacement,
    force: bool,
    regen: bool,
) -> anyhow::Result<()> {
    let Some(id) = tool.agent_config_id() else {
        anyhow::bail!("{} has no agent-config integration id", tool.display_name());
    };
    let installer = agent_config::instruction_by_id(id).ok_or_else(|| {
        anyhow::anyhow!(
            "{} does not support agent-config instructions",
            tool.display_name()
        )
    })?;
    let spec = agent_config::InstructionSpec::builder(name)
        .owner(SYNREPO_INSTALL_OWNER)
        .placement(placement)
        .body(tool.shim_spec_body())
        .adopt_unowned(force || regen)
        .try_build()?;
    let report = installer
        .install_instruction(scope, &spec)
        .map_err(|err| installer_error(tool, "instructions", err))?;
    print_agent_install_report(tool, "instructions", &report);
    Ok(())
}

fn installer_error(tool: AgentTool, label: &str, err: AgentConfigError) -> anyhow::Error {
    anyhow::Error::new(err).context(format!(
        "failed to install {} {} through agent-config",
        tool.display_name(),
        label
    ))
}

fn print_agent_install_report(tool: AgentTool, label: &str, report: &InstallReport) {
    if report.already_installed {
        println!("{} {label} is already current.", tool.display_name());
        return;
    }
    for path in &report.created {
        println!("Wrote {} {label}: {}", tool.display_name(), path.display());
    }
    for path in &report.patched {
        println!(
            "Updated {} {label}: {}",
            tool.display_name(),
            path.display()
        );
    }
    for path in &report.backed_up {
        println!("  Backup created: {}", path.display());
    }
}
