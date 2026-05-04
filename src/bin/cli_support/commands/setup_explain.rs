use anyhow::{anyhow, Context};
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

use super::setup::StepOutcome;

#[cfg(test)]
const TEST_GLOBAL_CONFIG_PATH_ENV: &str = "SYNREPO_TEST_GLOBAL_CONFIG_PATH";

const PROVIDER_KEY_FIELDS: &[&str] = &[
    "anthropic_api_key",
    "openai_api_key",
    "gemini_api_key",
    "openrouter_api_key",
    "zai_api_key",
    "minimax_api_key",
];

/// Patch repo-local `.synrepo/config.toml` plus user-scoped
/// `~/.synrepo/config.toml` based on the explain choice captured by the
/// wizard. Repo-local config owns opt-in and provider selection; user-scoped
/// config owns reusable API keys and local endpoints.
pub(crate) fn step_apply_explain(
    repo_root: &Path,
    choice: Option<&synrepo::tui::ExplainChoice>,
) -> anyhow::Result<StepOutcome> {
    use synrepo::tui::wizard::setup::explain::{CloudCredentialSource, ExplainChoice};

    let Some(choice) = choice else {
        println!("  Explain sub-wizard cancelled; repo and user config untouched.");
        return Ok(StepOutcome::AlreadyCurrent);
    };

    let local_path = repo_root.join(".synrepo").join("config.toml");
    let mut local_doc = load_toml_document(&local_path)?;
    let local_explain = ensure_table(&mut local_doc, &local_path, "explain")?;
    local_explain.set_implicit(false);
    local_explain.insert("enabled", Item::Value(TomlValue::from(true)));
    for field in PROVIDER_KEY_FIELDS {
        local_explain.remove(field);
    }
    local_explain.remove("local_endpoint");
    local_explain.remove("local_preset");

    match choice {
        ExplainChoice::Cloud { provider, .. } => {
            local_explain.insert(
                "provider",
                Item::Value(TomlValue::from(provider.config_value())),
            );
        }
        ExplainChoice::Local { .. } => {
            local_explain.insert("provider", Item::Value(TomlValue::from("local")));
        }
    }

    write_toml_document(&local_path, &local_doc)?;

    match choice {
        ExplainChoice::Cloud {
            provider,
            credential_source: CloudCredentialSource::EnteredGlobal,
            api_key,
        } => {
            let api_key = api_key
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "refusing to persist {} explain config: entered API key is missing",
                        provider.config_value()
                    )
                })?;
            let global_path = global_config_path()?;
            let mut global_doc = load_toml_document(&global_path)?;
            let global_explain = ensure_table(&mut global_doc, &global_path, "explain")?;
            global_explain.set_implicit(false);
            global_explain.insert(
                provider.api_key_field(),
                Item::Value(TomlValue::from(api_key)),
            );
            write_toml_document(&global_path, &global_doc)?;
            println!(
                "  Saved {} API key in {}",
                provider.config_value(),
                global_path.display()
            );
            println!(
                "  Warning: saved cloud API keys are plaintext in this file; prefer env vars on shared machines."
            );
        }
        ExplainChoice::Local { preset, endpoint } => {
            let global_path = global_config_path()?;
            let mut global_doc = load_toml_document(&global_path)?;
            let global_explain = ensure_table(&mut global_doc, &global_path, "explain")?;
            global_explain.set_implicit(false);
            global_explain.insert(
                "local_endpoint",
                Item::Value(TomlValue::from(endpoint.as_str())),
            );
            global_explain.insert(
                "local_preset",
                Item::Value(TomlValue::from(preset.config_value())),
            );
            write_toml_document(&global_path, &global_doc)?;
            println!(
                "  Saved local explain endpoint in {}",
                global_path.display()
            );
            println!(
                "  Warning: local explain endpoints receive source and context snippets during refresh."
            );
        }
        _ => {}
    }

    println!(
        "  Wrote repo-local [explain] block to {} (provider: {})",
        local_path.display(),
        match choice {
            ExplainChoice::Cloud { provider, .. } => provider.config_value(),
            ExplainChoice::Local { .. } => "local",
        }
    );
    Ok(StepOutcome::Applied)
}

fn global_config_path() -> anyhow::Result<std::path::PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os(TEST_GLOBAL_CONFIG_PATH_ENV) {
        return Ok(std::path::PathBuf::from(path));
    }

    synrepo::config::home_dir()
        .map(|home| home.join(".synrepo").join("config.toml"))
        .ok_or_else(|| anyhow!("cannot resolve home directory for ~/.synrepo/config.toml"))
}

fn load_toml_document(path: &Path) -> anyhow::Result<DocumentMut> {
    let raw = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };
    raw.parse().map_err(|err| {
        anyhow!(
            "refusing to overwrite {}: file exists but is not valid TOML ({err})",
            path.display()
        )
    })
}

fn ensure_table<'a>(
    doc: &'a mut DocumentMut,
    path: &Path,
    key: &str,
) -> anyhow::Result<&'a mut Table> {
    let item = doc.entry(key).or_insert_with(|| Item::Table(Table::new()));
    item.as_table_mut().ok_or_else(|| {
        anyhow!(
            "refusing to overwrite {}: `{key}` exists but is not a table",
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
