use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::io::Write as _;
use std::path::Path;
use synrepo::config::Mode;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

use super::basic::{agent_setup, init};
use crate::cli_support::agent_shims::AgentTool;

/// Outcome of a single setup step. Tests assert on this rather than captured
/// stdout; the CLI still prints progress lines for user-visible output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StepOutcome {
    /// Step performed a new write.
    Applied,
    /// Step was a no-op; existing state already matched the target.
    AlreadyCurrent,
    /// Step updated an existing value (present but different).
    Updated,
    /// Automation is not implemented for the given target.
    NotAutomated,
}

/// Full onboarding flow for a specific agent client. Thin composer over the
/// decomposed `step_*` helpers so TUI wizards can reuse the same steps.
pub(crate) fn setup(repo_root: &Path, tool: AgentTool, force: bool) -> anyhow::Result<()> {
    println!("Setting up synrepo for {}...", tool.display_name());

    step_init(repo_root, None, force)?;
    step_apply_integration(repo_root, tool, force)?;

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

/// Initialize `.synrepo/` if not present (or always with `force`). Returns
/// `AlreadyCurrent` when the directory is present and `force` is false.
pub(crate) fn step_init(
    repo_root: &Path,
    mode: Option<Mode>,
    force: bool,
) -> anyhow::Result<StepOutcome> {
    let synrepo_dir = repo_root.join(".synrepo");
    if !synrepo_dir.exists() || force {
        println!("  Initializing .synrepo/...");
        init(repo_root, mode)?;
        Ok(StepOutcome::Applied)
    } else {
        println!("  .synrepo/ already initialized.");
        Ok(StepOutcome::AlreadyCurrent)
    }
}

/// Write the agent integration shim for `target`. Delegates to the existing
/// `agent_setup` helper in regen mode so re-runs are idempotent.
pub(crate) fn step_write_shim(
    repo_root: &Path,
    target: AgentTool,
    force: bool,
) -> anyhow::Result<StepOutcome> {
    println!("  Writing integration shim...");
    agent_setup(repo_root, target, force, true)?;
    Ok(StepOutcome::Applied)
}

/// Register the synrepo MCP server in the target agent's project config.
/// Returns `NotAutomated` for targets without scripted registration.
pub(crate) fn step_register_mcp(
    repo_root: &Path,
    target: AgentTool,
) -> anyhow::Result<StepOutcome> {
    match target {
        AgentTool::Claude => setup_claude_mcp(repo_root),
        AgentTool::Codex => setup_codex_mcp(repo_root),
        AgentTool::OpenCode => setup_opencode_mcp(repo_root),
        _ => {
            println!(
                "  Note: Project-scoped MCP registration is not yet automated for {}.",
                target.display_name()
            );
            Ok(StepOutcome::NotAutomated)
        }
    }
}

/// Composite integration step: write the shim, then register the MCP server.
pub(crate) fn step_apply_integration(
    repo_root: &Path,
    target: AgentTool,
    force: bool,
) -> anyhow::Result<StepOutcome> {
    let shim = step_write_shim(repo_root, target, force)?;
    let mcp = step_register_mcp(repo_root, target)?;
    Ok(match (shim, mcp) {
        (StepOutcome::Applied, _) => StepOutcome::Applied,
        (_, StepOutcome::Applied) | (_, StepOutcome::Updated) => StepOutcome::Applied,
        (_, StepOutcome::NotAutomated) => StepOutcome::NotAutomated,
        _ => StepOutcome::AlreadyCurrent,
    })
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
    write_atomic(path, out.as_bytes())
}

/// Sibling temp file + fsync + rename. `fs::write`'s O_TRUNC can leave a
/// zero-length target behind on crash; rename-in-place cannot.
fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("invalid target path {}", path.display()))?
        .to_string_lossy();
    let tmp = parent.join(format!(
        ".{}.tmp.{}.{}",
        file_name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("failed to open temp file {}", tmp.display()))?;
        file.write_all(contents)
            .with_context(|| format!("failed to write temp file {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to fsync temp file {}", tmp.display()))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        anyhow!(
            "failed to rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
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
    write_json_config(&mcp_json_path, &config)?;
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
    write_json_config(&opencode_json_path, &config)?;
    println!("  Registered MCP server in opencode.json");
    Ok(prior)
}
