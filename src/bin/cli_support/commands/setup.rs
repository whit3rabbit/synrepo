use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use synrepo::config::Mode;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

use super::basic::{agent_setup, init};
use crate::cli_support::agent_shims::{AgentTool, AutomationTier};

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
pub(crate) fn setup(
    repo_root: &Path,
    tool: AgentTool,
    force: bool,
    gitignore: bool,
) -> anyhow::Result<()> {
    println!("Setting up synrepo for {}...", tool.display_name());

    step_init(repo_root, None, force, gitignore)?;
    step_apply_integration(repo_root, tool, force)?;
    step_ensure_ready(repo_root)?;

    println!("\nSetup complete. Repo is ready. One Next Step:");
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
        AgentTool::Cursor => {
            println!(
                "  Cursor will automatically load the synrepo MCP server from .cursor/mcp.json."
            )
        }
        AgentTool::Windsurf => {
            println!(
                "  Windsurf will automatically load the synrepo MCP server from .windsurf/mcp.json."
            )
        }
        AgentTool::Roo => {
            println!(
                "  Roo Code will automatically load the synrepo MCP server from .roo/mcp.json."
            )
        }
        other => {
            // Shim-only tier: the shim is written, but MCP registration is
            // manual. Give the operator the concrete follow-ups they need.
            debug_assert_eq!(other.automation_tier(), AutomationTier::ShimOnly);
            println!("  Shim written: {}", other.output_path(repo_root).display());
            println!("  Next: {}", other.include_instruction());
            println!("  MCP server: point your agent at `synrepo mcp --repo .` (stdio transport).");
        }
    }

    Ok(())
}

/// Initialize `.synrepo/` if not present (or always with `force`). Returns
/// `AlreadyCurrent` when the directory is present and `force` is false.
pub(crate) fn step_init(
    repo_root: &Path,
    mode: Option<Mode>,
    force: bool,
    gitignore: bool,
) -> anyhow::Result<StepOutcome> {
    let synrepo_dir = repo_root.join(".synrepo");
    if !synrepo_dir.exists() || force {
        println!("  Initializing .synrepo/...");
        init(repo_root, mode, gitignore)?;
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
        AgentTool::Cursor => setup_cursor_mcp(repo_root),
        AgentTool::Windsurf => setup_windsurf_mcp(repo_root),
        AgentTool::Roo => setup_roo_mcp(repo_root),
        other => {
            debug_assert_eq!(other.automation_tier(), AutomationTier::ShimOnly);
            println!(
                "  {} uses shim-only integration; register `synrepo mcp --repo .` \
                 as a stdio MCP server in the agent's own config.",
                other.display_name()
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

/// Patch `.synrepo/config.toml` with a `[synthesis]` block derived from
/// `choice`. `None` (user chose "Skip") is a no-op so re-running
/// `synrepo setup --synthesis` and cancelling does not clobber prior config.
///
/// The write is atomic and preserves unrelated keys/comments via `toml_edit`.
/// `local_endpoint` is written authoritatively; `local_preset` is informational.
pub(crate) fn step_apply_synthesis(
    repo_root: &Path,
    choice: Option<&synrepo::tui::SynthesisChoice>,
) -> anyhow::Result<StepOutcome> {
    use synrepo::tui::wizard::setup::synthesis::{CloudProvider, SynthesisChoice};

    let Some(choice) = choice else {
        println!("  Synthesis sub-wizard cancelled; config.toml untouched.");
        return Ok(StepOutcome::AlreadyCurrent);
    };

    let config_path = repo_root.join(".synrepo").join("config.toml");
    let raw = if config_path.exists() {
        fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?
    } else {
        String::new()
    };
    let mut doc: DocumentMut = raw.parse().map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid TOML ({err})",
            config_path.display()
        )
    })?;

    let synthesis_item = doc
        .entry("synthesis")
        .or_insert_with(|| Item::Table(Table::new()));
    let synthesis = synthesis_item.as_table_mut().ok_or_else(|| {
        anyhow!(
            "refusing to overwrite {}: `synthesis` exists but is not a table",
            config_path.display()
        )
    })?;
    synthesis.set_implicit(false);

    synthesis.insert("enabled", Item::Value(TomlValue::from(true)));
    match choice {
        SynthesisChoice::Cloud(provider) => {
            let name = match provider {
                CloudProvider::Anthropic => "anthropic",
                CloudProvider::OpenAi => "openai",
                CloudProvider::Gemini => "gemini",
            };
            synthesis.insert("provider", Item::Value(TomlValue::from(name)));
            synthesis.remove("local_endpoint");
            synthesis.remove("local_preset");
        }
        SynthesisChoice::Local { preset, endpoint } => {
            synthesis.insert("provider", Item::Value(TomlValue::from("local")));
            synthesis.insert(
                "local_endpoint",
                Item::Value(TomlValue::from(endpoint.as_str())),
            );
            synthesis.insert(
                "local_preset",
                Item::Value(TomlValue::from(preset.config_value())),
            );
        }
    }

    write_atomic(&config_path, doc.to_string().as_bytes())?;
    println!(
        "  Wrote [synthesis] block to {} (provider: {})",
        config_path.display(),
        match choice {
            SynthesisChoice::Cloud(p) => p.config_value(),
            SynthesisChoice::Local { .. } => "local",
        }
    );
    Ok(StepOutcome::Applied)
}

/// Ensure setup leaves an operationally ready runtime by creating the first
/// reconcile state when it is still missing after init.
pub(crate) fn step_ensure_ready(repo_root: &Path) -> anyhow::Result<StepOutcome> {
    let state_path = repo_root
        .join(".synrepo")
        .join("state")
        .join("reconcile-state.json");
    if state_path.exists() {
        println!("  Reconcile state already present.");
        return Ok(StepOutcome::AlreadyCurrent);
    }

    println!("  Running first reconcile pass...");
    super::repair::reconcile(repo_root)?;
    Ok(StepOutcome::Applied)
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

fn write_atomic(path: &Path, contents: &[u8]) -> anyhow::Result<()> {
    synrepo::util::atomic_write(path, contents)
        .with_context(|| format!("failed to atomically write {}", path.display()))
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

/// Standard `mcpServers.synrepo` entry shared by the Cursor/Windsurf/Roo
/// `.<tool>/mcp.json` editors. Claude Code's `.mcp.json` entry adds a `scope`
/// field, so it doesn't share this factory.
fn shim_tool_synrepo_entry() -> Value {
    json!({
        "command": "synrepo",
        "args": ["mcp", "--repo", "."],
    })
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
    write_json_config(&mcp_json_path, &config)?;
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
