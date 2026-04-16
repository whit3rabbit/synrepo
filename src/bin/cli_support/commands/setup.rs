use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

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

/// Target entry inserted under `mcpServers.synrepo` in `.mcp.json`.
fn claude_synrepo_entry() -> Value {
    json!({
        "command": "synrepo",
        "args": ["mcp", "--repo", "."],
        "scope": "project",
    })
}

/// Target entry inserted under `mcp.synrepo` in `.codex/config.toml`.
const CODEX_SYNREPO_VALUE: &str = "synrepo mcp --repo .";

/// Target entry inserted under `mcp.synrepo` in `opencode.json`.
fn opencode_synrepo_entry() -> Value {
    json!("synrepo mcp --repo .")
}

/// Parse a JSON file if it exists; fail loud with the file path if the content
/// is present but malformed, rather than silently discarding user config.
fn load_json_config(path: &Path) -> anyhow::Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str::<Value>(&content).map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid JSON ({err}). \
             Fix or remove the file and re-run `synrepo setup`.",
            path.display()
        )
    })
}

/// Write JSON back to disk with pretty-printing and a trailing newline.
fn write_json_config(path: &Path, value: &Value) -> anyhow::Result<()> {
    let mut out = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    out.push('\n');
    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn setup_claude_mcp(repo_root: &Path) -> anyhow::Result<()> {
    let mcp_json_path = repo_root.join(".mcp.json");
    let mut config = load_json_config(&mcp_json_path)?;

    // Preserve non-object roots by erroring rather than replacing silently.
    if !config.is_object() {
        return Err(anyhow!(
            "refusing to overwrite {}: root is not a JSON object",
            mcp_json_path.display()
        ));
    }

    let target = claude_synrepo_entry();

    // Ensure `mcpServers` is an object without clobbering unknown siblings.
    let root = config.as_object_mut().expect("object checked above");
    let servers_entry = root
        .entry("mcpServers".to_string())
        .or_insert_with(|| json!({}));
    let servers = servers_entry.as_object_mut().ok_or_else(|| {
        anyhow!(
            "refusing to overwrite {}: `mcpServers` exists but is not an object",
            mcp_json_path.display()
        )
    })?;

    match servers.get("synrepo") {
        Some(existing) if existing == &target => {
            println!(
                "  synrepo already registered in {} (no changes)",
                mcp_json_path.display()
            );
            return Ok(());
        }
        Some(_) => {
            println!(
                "  Updating existing synrepo entry in {}",
                mcp_json_path.display()
            );
        }
        None => {}
    }

    servers.insert("synrepo".to_string(), target);
    write_json_config(&mcp_json_path, &config)?;
    println!("  Registered project-scoped MCP server in .mcp.json");
    Ok(())
}

pub(crate) fn setup_codex_mcp(repo_root: &Path) -> anyhow::Result<()> {
    let codex_dir = repo_root.join(".codex");
    fs::create_dir_all(&codex_dir)
        .with_context(|| format!("failed to create config directory {}", codex_dir.display()))?;
    let config_path = codex_dir.join("config.toml");

    let raw = if config_path.exists() {
        fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?
    } else {
        String::new()
    };

    let mut doc: DocumentMut = raw.parse().map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid TOML ({err}). \
             Fix or remove the file and re-run `synrepo setup`.",
            config_path.display()
        )
    })?;

    // Ensure an `[mcp]` table exists as a real table. If a key named `mcp`
    // exists but is not a table, bail rather than overwrite user data.
    let mcp_item = doc
        .entry("mcp")
        .or_insert_with(|| Item::Table(Table::new()));
    let mcp_table = mcp_item.as_table_mut().ok_or_else(|| {
        anyhow!(
            "refusing to overwrite {}: `mcp` exists but is not a table",
            config_path.display()
        )
    })?;
    mcp_table.set_implicit(false);

    let already_current = mcp_table
        .get("synrepo")
        .and_then(|i| i.as_str())
        .map(|s| s == CODEX_SYNREPO_VALUE)
        .unwrap_or(false);

    if already_current {
        println!(
            "  synrepo already registered in {} (no changes)",
            config_path.display()
        );
        return Ok(());
    }

    if mcp_table.get("synrepo").is_some() {
        println!(
            "  Updating existing synrepo entry in {}",
            config_path.display()
        );
    }

    mcp_table.insert("synrepo", Item::Value(TomlValue::from(CODEX_SYNREPO_VALUE)));

    fs::write(&config_path, doc.to_string())
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    println!("  Registered MCP server in .codex/config.toml");
    Ok(())
}

pub(crate) fn setup_opencode_mcp(repo_root: &Path) -> anyhow::Result<()> {
    let opencode_json_path = repo_root.join("opencode.json");
    let mut config = load_json_config(&opencode_json_path)?;

    if !config.is_object() {
        return Err(anyhow!(
            "refusing to overwrite {}: root is not a JSON object",
            opencode_json_path.display()
        ));
    }

    let target = opencode_synrepo_entry();

    let root = config.as_object_mut().expect("object checked above");
    let mcp_entry = root.entry("mcp".to_string()).or_insert_with(|| json!({}));
    let mcp = mcp_entry.as_object_mut().ok_or_else(|| {
        anyhow!(
            "refusing to overwrite {}: `mcp` exists but is not an object",
            opencode_json_path.display()
        )
    })?;

    match mcp.get("synrepo") {
        Some(existing) if existing == &target => {
            println!(
                "  synrepo already registered in {} (no changes)",
                opencode_json_path.display()
            );
            return Ok(());
        }
        Some(_) => {
            println!(
                "  Updating existing synrepo entry in {}",
                opencode_json_path.display()
            );
        }
        None => {}
    }

    mcp.insert("synrepo".to_string(), target);
    write_json_config(&opencode_json_path, &config)?;
    println!("  Registered MCP server in opencode.json");
    Ok(())
}
