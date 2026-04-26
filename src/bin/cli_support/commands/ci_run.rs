use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use serde::Serialize;
use synrepo::config::Config;
use synrepo::structure::graph::GraphReader;
use synrepo::surface::card::{Budget, CardCompiler, ChangeRiskCard};

use crate::cli_support::cli_args::CiRunArgs;

#[derive(Clone, Debug)]
pub(crate) struct CiRunOptions {
    pub(crate) targets: Vec<String>,
    pub(crate) changed_from: Option<String>,
    pub(crate) budget: Option<String>,
    pub(crate) json: bool,
}

impl From<CiRunArgs> for CiRunOptions {
    fn from(args: CiRunArgs) -> Self {
        Self {
            targets: args.targets,
            changed_from: args.changed_from,
            budget: args.budget,
            json: args.json,
        }
    }
}

#[derive(Serialize)]
struct CiRunReport {
    mode: &'static str,
    store: &'static str,
    compile: synrepo::pipeline::structural::CompileSummary,
    targets: Vec<String>,
    unresolved_targets: Vec<String>,
    cards: Vec<ChangeRiskCard>,
}

pub(crate) fn ci_run(repo_root: &Path, args: CiRunArgs) -> anyhow::Result<()> {
    print!("{}", ci_run_output(repo_root, args.into())?);
    Ok(())
}

pub(crate) fn ci_run_output(repo_root: &Path, options: CiRunOptions) -> anyhow::Result<String> {
    let config = load_ci_config(repo_root)?;
    let mut store = synrepo::structure::graph::MemGraphStore::new();
    let compile =
        synrepo::pipeline::structural::run_structural_compile(repo_root, &config, &mut store)?;
    let graph = Arc::new(store.into_graph()?);
    let compiler = synrepo::surface::card::compiler::GraphCardCompiler::new_with_snapshot(
        graph.clone(),
        Some(repo_root),
    )
    .with_config(config);

    let budget = parse_budget(options.budget.as_deref())?;
    let mut targets = options.targets;
    if let Some(base) = options.changed_from.as_deref() {
        targets.extend(changed_file_targets(repo_root, base)?);
    }
    if targets.is_empty() {
        targets.extend(graph.all_file_paths()?.into_iter().map(|(path, _)| path));
    }
    targets.sort();
    targets.dedup();

    let mut cards = Vec::new();
    let mut unresolved_targets = Vec::new();
    for target in &targets {
        match compiler.resolve_target(target)? {
            Some(node_id) => cards.push(compiler.change_risk_card(node_id, budget)?),
            None => unresolved_targets.push(target.clone()),
        }
    }

    let report = CiRunReport {
        mode: "ci-run",
        store: "memory",
        compile,
        targets,
        unresolved_targets,
        cards,
    };

    if options.json {
        return Ok(format!("{}\n", serde_json::to_string_pretty(&report)?));
    }

    Ok(render_markdown(&report))
}

fn load_ci_config(repo_root: &Path) -> anyhow::Result<Config> {
    match Config::load(repo_root) {
        Ok(config) => Ok(config),
        Err(synrepo::Error::NotInitialized(_)) => Ok(Config::default()),
        Err(error) => Err(error.into()),
    }
}

fn parse_budget(value: Option<&str>) -> anyhow::Result<Budget> {
    match value {
        Some("tiny") | None => Ok(Budget::Tiny),
        Some("normal") => Ok(Budget::Normal),
        Some("deep") => Ok(Budget::Deep),
        Some(other) => anyhow::bail!("invalid budget: {other} (expected tiny, normal, or deep)"),
    }
}

fn changed_file_targets(repo_root: &Path, base: &str) -> anyhow::Result<Vec<String>> {
    let range = format!("{base}...HEAD");
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=ACMR", &range])
        .current_dir(repo_root)
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "git diff against {base} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn render_markdown(report: &CiRunReport) -> String {
    let mut out = String::new();
    out.push_str("## synrepo CI Run\n\n");
    out.push_str("- Store: memory\n");
    out.push_str(&format!(
        "- Compile: {} files discovered, {} parsed, {} symbols, {} edges\n",
        report.compile.files_discovered,
        report.compile.files_parsed,
        report.compile.symbols_extracted,
        report.compile.edges_added
    ));
    out.push_str(&format!("- Targets: {}\n", report.targets.len()));
    if !report.unresolved_targets.is_empty() {
        out.push_str(&format!(
            "- Unresolved: {}\n",
            report.unresolved_targets.join(", ")
        ));
    }

    out.push_str("\n### Change Risk\n\n");
    if report.cards.is_empty() {
        out.push_str("No risk cards produced.\n");
        return out;
    }

    out.push_str("| Target | Kind | Risk | Score |\n");
    out.push_str("|---|---:|---:|---:|\n");
    for card in &report.cards {
        out.push_str(&format!(
            "| `{}` | {} | {:?} | {:.2} |\n",
            card.target_name, card.target_kind, card.risk_level, card.risk_score
        ));
    }
    out
}
