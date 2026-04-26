//! Agent-specific MCP server registration functions.

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use toml_edit::{Array, DocumentMut, Item, Table, Value as TomlValue};

use super::config::{load_json_config, write_atomic};
use super::steps::StepOutcome;

/// Target entry inserted under `mcpServers.synrepo` in `.mcp.json`.
fn claude_synrepo_entry(global: bool) -> Value {
    if global {
        json!({
            "command": "synrepo",
            "args": ["mcp"],
        })
    } else {
        json!({
            "command": "synrepo",
            "args": ["mcp", "--repo", "."],
            "scope": "project",
        })
    }
}

/// Target entry inserted under `mcp_servers.synrepo` in `.codex/config.toml`.
const CODEX_SYNREPO_COMMAND: &str = "synrepo";
const CODEX_SYNREPO_ARGS: &[&str] = &["mcp", "--repo", "."];

/// Target entry inserted under `mcp.synrepo` in `opencode.json`.
fn opencode_synrepo_entry() -> Value {
    json!("synrepo mcp --repo .")
}

/// Standard `mcpServers.synrepo` entry shared by the Cursor/Windsurf/Roo
/// `.<tool>/mcp.json` editors. Claude Code's `.mcp.json` entry adds a `scope`
/// field, so it doesn't share this factory.
fn shim_tool_synrepo_entry(global: bool) -> Value {
    if global {
        json!({
            "command": "synrepo",
            "args": ["mcp"],
        })
    } else {
        json!({
            "command": "synrepo",
            "args": ["mcp", "--repo", "."],
        })
    }
}

pub(crate) fn setup_claude_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    let mcp_json_path = if global {
        let home =
            synrepo::config::home_dir().ok_or_else(|| anyhow!("Failed to find home directory"))?;
        let path = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Claude/claude_desktop_config.json")
        } else if cfg!(target_os = "windows") {
            std::env::var("APPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| home.join("AppData").join("Roaming"))
                .join("Claude/claude_desktop_config.json")
        } else {
            home.join(".config/Claude/claude_desktop_config.json")
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        path
    } else {
        repo_root.join(".mcp.json")
    };
    let mut config = load_json_config(&mcp_json_path)?;

    if !config.is_object() {
        return Err(anyhow!(
            "refusing to overwrite {}: root is not a JSON object",
            mcp_json_path.display()
        ));
    }

    let target = claude_synrepo_entry(global);

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
        "  Registered {} MCP server in {}",
        if global { "global" } else { "project-scoped" },
        mcp_json_path.display()
    );
    Ok(prior)
}

pub(crate) fn setup_codex_mcp(repo_root: &Path, _global: bool) -> anyhow::Result<StepOutcome> {
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

    let legacy_removed = remove_legacy_codex_mcp(&mut doc);
    let synrepo_table = ensure_codex_synrepo_table(&mut doc, &config_path)?;
    let already_current = codex_synrepo_table_current(synrepo_table);

    if already_current && !legacy_removed {
        println!(
            "  synrepo already registered in {} (no changes)",
            config_path.display()
        );
        return Ok(StepOutcome::AlreadyCurrent);
    }

    let prior = if already_current {
        println!(
            "  Removed legacy synrepo entry in {}",
            config_path.display()
        );
        StepOutcome::Updated
    } else if synrepo_table.get("command").is_some() || synrepo_table.get("args").is_some() {
        println!(
            "  Updating existing synrepo entry in {}",
            config_path.display()
        );
        StepOutcome::Updated
    } else {
        StepOutcome::Applied
    };

    write_codex_synrepo_table(synrepo_table);

    write_atomic(&config_path, doc.to_string().as_bytes())?;
    println!("  Registered MCP server in .codex/config.toml");
    Ok(prior)
}

fn ensure_codex_synrepo_table<'a>(
    doc: &'a mut DocumentMut,
    config_path: &Path,
) -> anyhow::Result<&'a mut Table> {
    if !doc.as_table().contains_key("mcp_servers") {
        doc.as_table_mut()
            .insert("mcp_servers", Item::Table(Table::new()));
    }
    let servers = doc
        .get_mut("mcp_servers")
        .and_then(|item| item.as_table_mut())
        .ok_or_else(|| {
            anyhow!(
                "refusing to overwrite {}: `mcp_servers` exists but is not a table",
                config_path.display()
            )
        })?;
    servers.set_implicit(false);

    if !servers.contains_key("synrepo") {
        servers.insert("synrepo", Item::Table(Table::new()));
    }
    let synrepo = servers
        .get_mut("synrepo")
        .and_then(|item| item.as_table_mut())
        .ok_or_else(|| {
            anyhow!(
                "refusing to overwrite {}: `mcp_servers.synrepo` exists but is not a table",
                config_path.display()
            )
        })?;
    synrepo.set_implicit(false);
    Ok(synrepo)
}

fn remove_legacy_codex_mcp(doc: &mut DocumentMut) -> bool {
    doc.get_mut("mcp")
        .and_then(|item| item.as_table_mut())
        .and_then(|table| table.remove("synrepo"))
        .is_some()
}

fn codex_synrepo_table_current(table: &Table) -> bool {
    let command_current = table
        .get("command")
        .and_then(|item| item.as_str())
        .map(|value| value == CODEX_SYNREPO_COMMAND)
        .unwrap_or(false);
    let args_current = table
        .get("args")
        .and_then(|item| item.as_array())
        .map(|args| {
            args.iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
                .as_slice()
                == CODEX_SYNREPO_ARGS
        })
        .unwrap_or(false);
    command_current && args_current
}

fn write_codex_synrepo_table(table: &mut Table) {
    let mut args = Array::default();
    for arg in CODEX_SYNREPO_ARGS {
        args.push(*arg);
    }
    table.insert(
        "command",
        Item::Value(TomlValue::from(CODEX_SYNREPO_COMMAND)),
    );
    table.insert("args", Item::Value(TomlValue::Array(args)));
}

pub(crate) fn setup_opencode_mcp(repo_root: &Path, _global: bool) -> anyhow::Result<StepOutcome> {
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
    global: bool,
) -> anyhow::Result<StepOutcome> {
    let config_dir = if global {
        synrepo::config::home_dir()
            .ok_or_else(|| anyhow!("Failed to find home directory"))?
            .join(config_dir_name)
    } else {
        repo_root.join(config_dir_name)
    };
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

    let target = shim_tool_synrepo_entry(global);

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
        "  Registered {} MCP server in {}",
        if global { "global" } else { "project-scoped" },
        mcp_json_path.display()
    );
    Ok(prior)
}

pub(crate) fn setup_cursor_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    register_mcp_servers_json(repo_root, ".cursor", global)
}

pub(crate) fn setup_windsurf_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    register_mcp_servers_json(repo_root, ".windsurf", global)
}

pub(crate) fn setup_roo_mcp(repo_root: &Path, global: bool) -> anyhow::Result<StepOutcome> {
    register_mcp_servers_json(repo_root, ".roo", global)
}
