//! synrepo CLI entry point.
//!
//! Phase 0/1 subcommands:
//! - `synrepo init [--mode auto|curated]` — create `.synrepo/` in the current repo,
//!   inspect rationale sources to pick a bootstrap mode when one is not explicit,
//!   and build the current syntext-backed substrate index
//! - `synrepo search <query>` — exact lexical search against that persisted index
//! - `synrepo graph query <q>` — structured graph query (phase 1)
//! - `synrepo node <id>` — dump a node's metadata (phase 1)
//!
//! Card-returning subcommands (`synrepo card`, `synrepo where-to-edit`, etc.)
//! land in phase 2 alongside the MCP server.

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use walkdir::WalkDir;

use synrepo::config::{Config, Mode};

#[derive(Parser)]
#[command(name = "synrepo")]
#[command(about = "A context compiler for AI coding agents", long_about = None)]
#[command(version)]
struct Cli {
    /// Override the repo root. Defaults to the current directory.
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a `.synrepo/` directory in the current repo.
    Init {
        /// Operational mode.
        #[arg(long, value_enum)]
        mode: Option<ModeArg>,
    },

    /// Lexical search via the syntext index.
    Search {
        /// The query string.
        query: String,
    },

    /// Graph-level queries and inspection.
    #[command(subcommand)]
    Graph(GraphCommand),

    /// Dump a node's metadata by ID.
    Node {
        /// The node ID in display format (e.g. `file_0000000000000042`).
        id: String,
    },
}

#[derive(Subcommand)]
enum GraphCommand {
    /// Run a structured query against the graph store.
    Query {
        /// The query string.
        q: String,
    },

    /// Print graph statistics (node count by type, edge count by kind).
    Stats,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ModeArg {
    Auto,
    Curated,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Auto => Mode::Auto,
            ModeArg::Curated => Mode::Curated,
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let repo_root = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    match cli.command {
        Command::Init { mode } => init(&repo_root, mode.map(Into::into)),
        Command::Search { query } => search(&repo_root, &query),
        Command::Graph(GraphCommand::Query { q }) => graph_query(&repo_root, &q),
        Command::Graph(GraphCommand::Stats) => graph_stats(&repo_root),
        Command::Node { id } => node(&repo_root, &id),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BootstrapHealth {
    Healthy,
    Degraded,
}

impl BootstrapHealth {
    fn as_str(self) -> &'static str {
        match self {
            BootstrapHealth::Healthy => "healthy",
            BootstrapHealth::Degraded => "degraded",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BootstrapAction {
    Created,
    Refreshed,
    Repaired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BootstrapReport {
    health: BootstrapHealth,
    mode: Mode,
    mode_guidance: Option<String>,
    synrepo_dir: PathBuf,
    substrate_status: String,
    next_step: String,
}

impl BootstrapReport {
    fn render(&self) -> String {
        let mut rendered = format!(
            "Bootstrap health: {}\nMode: {:?}\n",
            self.health.as_str(),
            self.mode,
        );
        if let Some(guidance) = &self.mode_guidance {
            rendered.push_str(&format!("Mode guidance: {}\n", guidance));
        }
        rendered.push_str(&format!(
            "Runtime path: {}\nSubstrate: {}\nNext: {}\n",
            self.synrepo_dir.display(),
            self.substrate_status,
            self.next_step
        ));
        rendered
    }
}

fn init(repo_root: &std::path::Path, requested_mode: Option<Mode>) -> anyhow::Result<()> {
    let report = bootstrap(repo_root, requested_mode)?;
    print!("{}", report.render());
    Ok(())
}

fn bootstrap(repo_root: &Path, requested_mode: Option<Mode>) -> anyhow::Result<BootstrapReport> {
    let synrepo_dir = Config::synrepo_dir(repo_root);
    let runtime_already_existed = synrepo_dir.exists();
    let config_path = synrepo_dir.join("config.toml");
    let existing_config = load_existing_config(&config_path, &synrepo_dir)?;
    let had_existing_config = existing_config.is_some();
    let gitignore_path = synrepo_dir.join(".gitignore");
    let had_gitignore = gitignore_path.exists();
    let inspection_config = existing_config.clone().unwrap_or_default();
    let inspection = inspect_repository_mode(repo_root, &inspection_config)?;
    let mode = requested_mode
        .or(existing_config.as_ref().map(|config| config.mode))
        .unwrap_or(inspection.recommended_mode);
    let mode_guidance = inspection.guidance_for(requested_mode, existing_config.as_ref(), mode);

    let layout_repaired = ensure_runtime_layout(&synrepo_dir, runtime_already_existed)?;
    let config = existing_config.unwrap_or_else(|| Config {
        mode,
        ..Config::default()
    });
    let config = Config { mode, ..config };

    std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
    write_synrepo_gitignore(&synrepo_dir)?;
    let repaired =
        runtime_already_existed && (layout_repaired || !had_existing_config || !had_gitignore);
    let action = if !runtime_already_existed {
        BootstrapAction::Created
    } else if repaired {
        BootstrapAction::Repaired
    } else {
        BootstrapAction::Refreshed
    };

    let build_report = synrepo::substrate::build_index(&config, repo_root)?;
    let health = match action {
        BootstrapAction::Repaired => BootstrapHealth::Degraded,
        BootstrapAction::Created | BootstrapAction::Refreshed => BootstrapHealth::Healthy,
    };
    let substrate_status = match action {
        BootstrapAction::Created => format!(
            "built initial index with {} discovered files",
            build_report.indexed_files
        ),
        BootstrapAction::Refreshed => format!(
            "refreshed existing index with {} discovered files",
            build_report.indexed_files
        ),
        BootstrapAction::Repaired => format!(
            "repaired runtime state and rebuilt index with {} discovered files",
            build_report.indexed_files
        ),
    };
    let next_step = match health {
        BootstrapHealth::Healthy => {
            "run `synrepo search <query>` to inspect the lexical index".to_string()
        }
        BootstrapHealth::Degraded => {
            "review the repaired runtime state, then run `synrepo search <query>`".to_string()
        }
    };

    Ok(BootstrapReport {
        health,
        mode,
        mode_guidance,
        synrepo_dir,
        substrate_status,
        next_step,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModeInspection {
    recommended_mode: Mode,
    rationale_dirs: Vec<PathBuf>,
}

impl ModeInspection {
    fn guidance_for(
        &self,
        requested_mode: Option<Mode>,
        existing_config: Option<&Config>,
        final_mode: Mode,
    ) -> Option<String> {
        if self.rationale_dirs.is_empty() {
            return match (requested_mode, existing_config) {
                (None, None) => Some(
                    "no rationale markdown was found under configured concept directories, so bootstrap defaulted to Auto.".to_string(),
                ),
                _ => None,
            };
        }

        let rationale_dirs = display_paths(&self.rationale_dirs);
        match (requested_mode, existing_config) {
            (Some(explicit_mode), _) if explicit_mode != self.recommended_mode => Some(format!(
                "repository inspection suggests {:?} because rationale markdown was found in {}; keeping explicit {:?}.",
                self.recommended_mode, rationale_dirs, explicit_mode
            )),
            (None, Some(config)) if config.mode != self.recommended_mode => Some(format!(
                "repository inspection suggests {:?} because rationale markdown was found in {}; keeping configured {:?}. Rerun `synrepo init --mode {}` to switch.",
                self.recommended_mode,
                rationale_dirs,
                config.mode,
                mode_flag(self.recommended_mode)
            )),
            (None, None) if final_mode == self.recommended_mode => Some(format!(
                "repository inspection selected {:?} because rationale markdown was found in {}.",
                final_mode, rationale_dirs
            )),
            _ => None,
        }
    }
}

fn inspect_repository_mode(repo_root: &Path, config: &Config) -> anyhow::Result<ModeInspection> {
    let rationale_dirs = config
        .concept_directories
        .iter()
        .filter_map(|relative_dir| {
            let dir = repo_root.join(relative_dir);
            if !dir.exists() {
                return None;
            }
            Some((relative_dir, dir))
        })
        .filter_map(|(relative_dir, dir)| match contains_markdown(&dir) {
            Ok(true) => Some(Ok(PathBuf::from(relative_dir))),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let recommended_mode = if rationale_dirs.is_empty() {
        Mode::Auto
    } else {
        Mode::Curated
    };

    Ok(ModeInspection {
        recommended_mode,
        rationale_dirs,
    })
}

fn contains_markdown(path: &Path) -> anyhow::Result<bool> {
    if path.is_file() {
        return Ok(is_markdown_path(path));
    }

    for entry in WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() && is_markdown_path(entry.path()) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext, "md" | "mdx" | "markdown"))
        .unwrap_or(false)
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn mode_flag(mode: Mode) -> &'static str {
    match mode {
        Mode::Auto => "auto",
        Mode::Curated => "curated",
    }
}

fn load_existing_config(config_path: &Path, synrepo_dir: &Path) -> anyhow::Result<Option<Config>> {
    if !config_path.exists() {
        return Ok(None);
    }

    fn blocked(synrepo_dir: &Path, issue: String, next: &str) -> anyhow::Error {
        anyhow::anyhow!(
            "Bootstrap health: blocked\nRuntime path: {}\nIssue: {issue}\nNext: {next}",
            synrepo_dir.display(),
        )
    }

    let text = std::fs::read_to_string(config_path).map_err(|error| {
        blocked(
            synrepo_dir,
            format!("failed to read existing config: {error}"),
            &format!("fix or remove {} and rerun `synrepo init`.", config_path.display()),
        )
    })?;
    toml::from_str(&text).map(Some).map_err(|error| {
        blocked(
            synrepo_dir,
            format!("invalid existing config at {}: {error}", config_path.display()),
            "fix or remove it, then rerun `synrepo init`.",
        )
    })
}

fn ensure_runtime_layout(
    synrepo_dir: &Path,
    runtime_already_existed: bool,
) -> anyhow::Result<bool> {
    let expected_directories = [
        synrepo_dir.to_path_buf(),
        synrepo_dir.join("graph"),
        synrepo_dir.join("overlay"),
        synrepo_dir.join("index"),
        synrepo_dir.join("embeddings"),
        synrepo_dir.join("cache/llm-responses"),
        synrepo_dir.join("state"),
    ];

    let mut any_missing = false;
    for directory in expected_directories {
        if !directory.exists() {
            any_missing = true;
            std::fs::create_dir_all(&directory)?;
        }
    }

    Ok(runtime_already_existed && any_missing)
}

fn write_synrepo_gitignore(synrepo_dir: &Path) -> anyhow::Result<()> {
    // Write a default .gitignore for .synrepo/
    let gitignore_path = synrepo_dir.join(".gitignore");
    std::fs::write(
        &gitignore_path,
        "# Gitignore everything in .synrepo/ except config.toml\n\
         *\n\
         !.gitignore\n\
         !config.toml\n",
    )?;
    Ok(())
}

fn search(repo_root: &std::path::Path, query: &str) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let matches = synrepo::substrate::search(&config, repo_root, query)?;

    for m in &matches {
        println!(
            "{}:{}: {}",
            m.path.display(),
            m.line_number,
            String::from_utf8_lossy(&m.line_content).trim_end()
        );
    }

    println!("Found {} matches.", matches.len());
    Ok(())
}

fn graph_query(_repo_root: &std::path::Path, _q: &str) -> anyhow::Result<()> {
    // TODO(phase-1)
    anyhow::bail!("graph query not yet implemented (phase 1 pending)")
}

fn graph_stats(_repo_root: &std::path::Path) -> anyhow::Result<()> {
    // TODO(phase-1)
    anyhow::bail!("graph stats not yet implemented (phase 1 pending)")
}

fn node(_repo_root: &std::path::Path, _id: &str) -> anyhow::Result<()> {
    // TODO(phase-1)
    anyhow::bail!("node lookup not yet implemented (phase 1 pending)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_fresh_repo_reports_healthy_summary() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "fresh token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();

        assert_eq!(report.health, BootstrapHealth::Healthy);
        assert_eq!(report.mode, Mode::Auto);
        assert!(rendered.contains("Bootstrap health: healthy"));
        assert!(rendered.contains("Mode: Auto"));
        assert!(rendered.contains("Mode guidance: no rationale markdown was found"));
        assert!(rendered.contains("Runtime path:"));
        assert!(rendered.contains("Substrate: built initial index"));
        assert!(rendered.contains("Next: run `synrepo search <query>`"));
    }

    #[test]
    fn bootstrap_selects_curated_when_rationale_markdown_exists() {
        let repo = tempdir().unwrap();
        let adr_dir = repo.path().join("docs/adr");
        std::fs::create_dir_all(&adr_dir).unwrap();
        std::fs::write(adr_dir.join("0001-record.md"), "# Decision\n").unwrap();
        std::fs::write(repo.path().join("README.md"), "curated token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();
        let config = Config::load(repo.path()).unwrap();

        assert_eq!(report.mode, Mode::Curated);
        assert_eq!(config.mode, Mode::Curated);
        assert!(rendered.contains("Mode guidance: repository inspection selected Curated"));
        assert!(rendered.contains("docs/adr"));
    }

    #[test]
    fn bootstrap_rerun_refreshes_existing_runtime() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "before refresh\n").unwrap();
        bootstrap(repo.path(), None).unwrap();

        std::fs::write(repo.path().join("README.md"), "after refresh token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let matches = synrepo::substrate::search(
            &Config::load(repo.path()).unwrap(),
            repo.path(),
            "after refresh token",
        )
        .unwrap();

        assert_eq!(report.health, BootstrapHealth::Healthy);
        assert!(report.substrate_status.contains("refreshed existing index"));
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn bootstrap_repairs_partial_runtime_and_reports_degraded() {
        let repo = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(repo.path());
        std::fs::create_dir_all(&synrepo_dir).unwrap();
        std::fs::write(repo.path().join("README.md"), "repair token\n").unwrap();

        let report = bootstrap(repo.path(), None).unwrap();
        let rendered = report.render();

        assert_eq!(report.health, BootstrapHealth::Degraded);
        assert!(rendered.contains("Bootstrap health: degraded"));
        assert!(rendered.contains("repaired runtime state and rebuilt index"));
        assert!(synrepo_dir.join("index/manifest.json").exists());
    }

    #[test]
    fn bootstrap_blocks_on_invalid_existing_config() {
        let repo = tempdir().unwrap();
        let synrepo_dir = Config::synrepo_dir(repo.path());
        std::fs::create_dir_all(&synrepo_dir).unwrap();
        std::fs::write(synrepo_dir.join("config.toml"), "mode = [not valid").unwrap();

        let error = bootstrap(repo.path(), None).unwrap_err().to_string();

        assert!(error.contains("Bootstrap health: blocked"));
        assert!(error.contains("invalid existing config"));
    }

    #[test]
    fn bootstrap_explicit_mode_overrides_existing_config_on_refresh() {
        let repo = tempdir().unwrap();
        std::fs::write(repo.path().join("README.md"), "mode token\n").unwrap();
        bootstrap(repo.path(), Some(Mode::Curated)).unwrap();

        let report = bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
        let config = Config::load(repo.path()).unwrap();

        assert_eq!(report.mode, Mode::Auto);
        assert_eq!(config.mode, Mode::Auto);
    }

    #[test]
    fn bootstrap_honors_explicit_auto_with_curated_recommendation() {
        let repo = tempdir().unwrap();
        let adr_dir = repo.path().join("docs/adr");
        std::fs::create_dir_all(&adr_dir).unwrap();
        std::fs::write(adr_dir.join("0002-architecture.md"), "# Architecture\n").unwrap();
        std::fs::write(repo.path().join("README.md"), "explicit token\n").unwrap();

        let report = bootstrap(repo.path(), Some(Mode::Auto)).unwrap();
        let rendered = report.render();
        let config = Config::load(repo.path()).unwrap();

        assert_eq!(report.mode, Mode::Auto);
        assert_eq!(config.mode, Mode::Auto);
        assert!(rendered.contains("Mode guidance: repository inspection suggests Curated"));
        assert!(rendered.contains("keeping explicit Auto"));
    }
}
