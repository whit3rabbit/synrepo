//! Agent-specific MCP server registration functions.

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

use super::config::{load_json_config, write_atomic};
use super::steps::StepOutcome;

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

/// Standard `mcpServers.synrepo` entry shared by the Cursor/Windsurf/Roo
/// `.<tool>/mcp.json` editors. Claude Code's `.mcp.json` entry adds a `scope`
/// field, so it doesn't share this factory.
fn shim_tool_synrepo_entry() -> Value {
    json!({
        "command": "synrepo",
        "args": ["mcp", "--repo", "."],
    })
}

pub(crate) fn setup_claude_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    let mcp_json_path = repo_root.join(".mcp.json");
    let mut config = load_json_config(&mcp_json_path)?;

    if !config.is_object() {
        return Err(anyhow!(
            "refusing to overwrite {}: root is not a JSON object",
            mcp_json_path.display()
        ));
    }

    let target = claude_synrepo_entry();

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

    let prior = match servers.get("synrepo") {
        Some(existing) if existing == &target => {
            println!(
                "  synrepo already registered in {} (no changes)",
                mcp_json_path.display()
            );
            return Ok(StepOutcome::AlreadyCurrent);
        }
        Some(_) => {
            println!(
                "  Updating existing synrepo entry in {}",
                mcp_json_path.display()
            );
            StepOutcome::Updated
        }
        None => StepOutcome::Applied,
    };

    servers.insert("synrepo".to_string(), target);
    super::write_json_config(&mcp_json_path, &config)?;
    println!("  Registered project-scoped MCP server in .mcp.json");
    Ok(prior)
}

pub(crate) fn setup_codex_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
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
        return Ok(StepOutcome::AlreadyCurrent);
    }

    let prior = if mcp_table.get("synrepo").is_some() {
        println!(
            "  Updating existing synrepo entry in {}",
            config_path.display()
        );
        StepOutcome::Updated
    } else {
        StepOutcome::Applied
    };

    mcp_table.insert("synrepo", Item::Value(TomlValue::from(CODEX_SYNREPO_VALUE)));

    write_atomic(&config_path, doc.to_string().as_bytes())?;
    println!("  Registered MCP server in .codex/config.toml");
    Ok(prior)
}

pub(crate) fn setup_opencode_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
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

    let prior = match mcp.get("synrepo") {
        Some(existing) if existing == &target => {
            println!(
                "  synrepo already registered in {} (no changes)",
                opencode_json_path.display()
            );
            return Ok(StepOutcome::AlreadyCurrent);
        }
        Some(_) => {
            println!(
                "  Updating existing synrepo entry in {}",
                opencode_json_path.display()
            );
            StepOutcome::Updated
        }
        None => StepOutcome::Applied,
    };

    mcp.insert("synrepo".to_string(), target);
    super::write_json_config(&opencode_json_path, &config)?;
    println!("  Registered MCP server in opencode.json");
    Ok(prior)
}

/// Edit `<config_dir_name>/mcp.json` to register synrepo under
/// `mcpServers.synrepo`. Shared by Cursor, Windsurf, and Roo; each uses the
/// same schema (a top-level `mcpServers` map keyed by server name).
fn register_mcp_servers_json(
    repo_root: &Path,
    config_dir_name: &str,
) -> anyhow::Result<StepOutcome> {
    let config_dir = repo_root.join(config_dir_name);
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("failed to create config directory {}", config_dir.display()))?;
    let mcp_json_path = config_dir.join("mcp.json");
    let mut config = load_json_config(&mcp_json_path)?;

    if !config.is_object() {
        return Err(anyhow!(
            "refusing to overwrite {}: root is not a JSON object",
            mcp_json_path.display()
        ));
    }

    let target = shim_tool_synrepo_entry();

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

    let prior = match servers.get("synrepo") {
        Some(existing) if existing == &target => {
            println!(
                "  synrepo already registered in {} (no changes)",
                mcp_json_path.display()
            );
            return Ok(StepOutcome::AlreadyCurrent);
        }
        Some(_) => {
            println!(
                "  Updating existing synrepo entry in {}",
                mcp_json_path.display()
            );
            StepOutcome::Updated
        }
        None => StepOutcome::Applied,
    };

    servers.insert("synrepo".to_string(), target);
    super::write_json_config(&mcp_json_path, &config)?;
    println!(
        "  Registered project-scoped MCP server in {}/mcp.json",
        config_dir_name
    );
    Ok(prior)
}

pub(crate) fn setup_cursor_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    register_mcp_servers_json(repo_root, ".cursor")
}

pub(crate) fn setup_windsurf_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    register_mcp_servers_json(repo_root, ".windsurf")
}

pub(crate) fn setup_roo_mcp(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    register_mcp_servers_json(repo_root, ".roo")
}
