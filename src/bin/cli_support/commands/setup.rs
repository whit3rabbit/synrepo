use anyhow::Context;
use serde_json::json;
use std::fs;
use std::path::Path;

use super::basic::{agent_setup, init};
use crate::cli_support::agent_shims::AgentTool;

/// full onboarding flow for a specific agent client.
pub(crate) fn setup(repo_root: &Path, tool: AgentTool, force: bool) -> anyhow::Result<()> {
    println!("Setting up synrepo for {}...", tool.display_name());

    // 1. Initialize synrepo if needed.
    let synrepo_dir = repo_root.join(".synrepo");
    if !synrepo_dir.exists() || force {
        println!("  Initializing .synrepo/...");
        init(repo_root, None)?;
    } else {
        println!("  .synrepo/ already initialized.");
    }

    // 2. Write client-specific instructions/shim.
    println!("  Writing integration shim...");
    agent_setup(repo_root, tool, force, true)?;

    // 3. Register MCP server in local config.
    match tool {
        AgentTool::Claude => setup_claude_mcp(repo_root)?,
        AgentTool::Codex => setup_codex_mcp(repo_root)?,
        AgentTool::OpenCode => setup_opencode_mcp(repo_root)?,
        _ => {
            println!(
                "  Note: Project-scoped MCP registration is not yet automated for {}.",
                tool.display_name()
            );
        }
    }

    println!("\nSetup complete! One Next Step:");
    match tool {
        AgentTool::Claude => {
            println!("  Run `claude` and it will automatically load the synrepo MCP server.")
        }
        AgentTool::Codex => {
            println!("  Run `codex` and it will automatically load the synrepo MCP server.")
        }
        AgentTool::OpenCode => {
            println!("  OpenCode will automatically load the synrepo MCP server and AGENTS.md.")
        }
        _ => {
            println!("  Configure your agent to use `synrepo mcp --repo .` as a stdio MCP server.")
        }
    }

    if fs::metadata(repo_root.join(".synrepo/state/reconcile-state.json")).is_err() {
        println!("\nTip: Run `synrepo reconcile` to build your first graph if `init` was empty.");
    }

    Ok(())
}

fn setup_claude_mcp(repo_root: &Path) -> anyhow::Result<()> {
    let mcp_json_path = repo_root.join(".mcp.json");
    let mut config = if mcp_json_path.exists() {
        let content = fs::read_to_string(&mcp_json_path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(json!({}))
    } else {
        json!({})
    };

    let servers = config
        .as_object_mut()
        .and_then(|o| o.get_mut("mcpServers"))
        .and_then(|v| v.as_object_mut());

    let synrepo_config = json!({
        "command": "synrepo",
        "args": ["mcp", "--repo", "."],
        "scope": "project"
    });

    if let Some(s) = servers {
        s.insert("synrepo".to_string(), synrepo_config);
    } else {
        config["mcpServers"] = json!({ "synrepo": synrepo_config });
    }

    let updated = serde_json::to_string_pretty(&config)?;
    fs::write(&mcp_json_path, updated).context("failed to write .mcp.json")?;
    println!("  Registered project-scoped MCP server in .mcp.json");
    Ok(())
}

fn setup_codex_mcp(repo_root: &Path) -> anyhow::Result<()> {
    let codex_dir = repo_root.join(".codex");
    if !codex_dir.exists() {
        fs::create_dir_all(&codex_dir)?;
    }
    let config_path = codex_dir.join("config.toml");

    let mut content = if config_path.exists() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    if !content.contains("[mcp]") {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("\n[mcp]\n");
    }

    if !content.contains("synrepo =") {
        content.push_str("synrepo = \"synrepo mcp --repo .\"\n");
        fs::write(&config_path, content).context("failed to write .codex/config.toml")?;
        println!("  Registered MCP server in .codex/config.toml");
    } else {
        println!("  synrepo already registered in .codex/config.toml");
    }
    Ok(())
}

fn setup_opencode_mcp(repo_root: &Path) -> anyhow::Result<()> {
    let opencode_json_path = repo_root.join("opencode.json");
    let mut config = if opencode_json_path.exists() {
        let content = fs::read_to_string(&opencode_json_path)?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or(json!({}))
    } else {
        json!({})
    };

    let mcp = config
        .as_object_mut()
        .and_then(|o| o.get_mut("mcp"))
        .and_then(|v| v.as_object_mut());

    if let Some(m) = mcp {
        m.insert("synrepo".to_string(), json!("synrepo mcp --repo ."));
    } else {
        config["mcp"] = json!({ "synrepo": "synrepo mcp --repo ." });
    }

    let updated = serde_json::to_string_pretty(&config)?;
    fs::write(&opencode_json_path, updated).context("failed to write opencode.json")?;
    println!("  Registered MCP server in opencode.json");
    Ok(())
}
