use std::path::Path;

use synrepo::tui::{stdout_is_tty, TuiOptions};

use super::cli_args::SetupArgs;
use super::commands::{resolve_tool_resolution, setup_many_resolved};
use super::setup_cmd;

pub(crate) fn dispatch_setup(
    repo_root: &Path,
    args: SetupArgs,
    tui_opts: TuiOptions,
) -> anyhow::Result<()> {
    let SetupArgs {
        tool,
        only,
        skip,
        force,
        explain,
        gitignore,
        project,
        agent_hooks,
        global,
    } = args;

    if global {
        eprintln!(
            "warning: `synrepo setup --global` is deprecated; global setup is now the default"
        );
    }
    let any_target = tool.is_some() || !only.is_empty() || !skip.is_empty();
    if any_target {
        let resolution = resolve_tool_resolution(tool, &only, &skip)?;
        setup_many_resolved(
            repo_root,
            &resolution,
            force,
            gitignore,
            project,
            agent_hooks,
        )?;
        if explain {
            setup_cmd::run_explain_step(repo_root, tui_opts)?;
        }
        return Ok(());
    }

    if explain_only_setup_requested(force, explain, gitignore, project, agent_hooks, global) {
        setup_cmd::run_explain_step(repo_root, tui_opts)?;
        return Ok(());
    }

    let mut bad_flags = Vec::new();
    if force {
        bad_flags.push("--force");
    }
    if explain {
        bad_flags.push("--explain");
    }
    if gitignore {
        bad_flags.push("--gitignore");
    }
    if project {
        bad_flags.push("--project");
    }
    if agent_hooks {
        bad_flags.push("--agent-hooks");
    }
    if global {
        bad_flags.push("--global");
    }
    if !bad_flags.is_empty() {
        anyhow::bail!(
            "`synrepo setup` without a tool launches the interactive wizard; \
             {} only applies when a tool is passed (e.g. `synrepo setup claude {}`).",
            bad_flags.join(" / "),
            bad_flags.join(" "),
        );
    }
    if !stdout_is_tty() {
        eprintln!(
            "synrepo setup: interactive wizard requires a TTY. \
             Pass a tool for the scripted flow (e.g. `synrepo setup claude`)."
        );
        std::process::exit(2);
    }
    setup_cmd::run_wizard_and_apply(repo_root, tui_opts)
}

fn explain_only_setup_requested(
    force: bool,
    explain: bool,
    gitignore: bool,
    project: bool,
    agent_hooks: bool,
    global: bool,
) -> bool {
    explain && !force && !gitignore && !project && !agent_hooks && !global
}

#[cfg(test)]
mod tests {
    use super::explain_only_setup_requested;

    #[test]
    fn setup_explain_without_tool_is_an_explain_only_setup_request() {
        assert!(explain_only_setup_requested(
            false, true, false, false, false, false
        ));
    }

    #[test]
    fn setup_explain_with_other_setup_flags_stays_invalid_without_tool() {
        assert!(!explain_only_setup_requested(
            true, true, false, false, false, false
        ));
        assert!(!explain_only_setup_requested(
            false, true, true, false, false, false
        ));
        assert!(!explain_only_setup_requested(
            false, true, false, true, false, false
        ));
        assert!(!explain_only_setup_requested(
            false, true, false, false, true, false
        ));
    }
}
