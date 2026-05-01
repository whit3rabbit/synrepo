use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::export::{write_exports, ExportFormat},
    pipeline::writer::{acquire_write_admission, map_lock_error},
    surface::card::Budget,
};

/// Generate export files in the configured export directory.
pub(crate) fn export(
    repo_root: &Path,
    format: ExportFormat,
    deep: bool,
    commit: bool,
    out: Option<String>,
) -> anyhow::Result<()> {
    let mut config = Config::load(repo_root).map_err(|e| {
        anyhow::anyhow!("export: not initialized, run `synrepo init --mode auto` first ({e})")
    })?;
    let synrepo_dir = Config::synrepo_dir(repo_root);

    // Hold the writer lock for the full duration: prevents the graph epoch
    // from advancing mid-export and producing a manifest inconsistent with
    // the rendered cards, and blocks concurrent writers that could leave
    // partial output in the export directory or .gitignore.
    let _writer_lock = acquire_write_admission(&synrepo_dir, "export")
        .map_err(|err| map_lock_error("export", err))?;

    if let Some(out_dir) = out {
        config.export_dir = out_dir;
    }

    let budget = if deep { Budget::Deep } else { Budget::Normal };

    let result = write_exports(repo_root, &synrepo_dir, &config, format, budget, commit)
        .map_err(|e| anyhow::anyhow!("export failed: {e}"))?;

    println!(
        "Export complete: {} files, {} symbols, {} decisions",
        result.file_count, result.symbol_count, result.decision_count
    );
    println!("  Directory: {}", result.export_dir.display());
    println!("  Format:    {}", result.manifest.format.as_str());
    println!("  Budget:    {}", result.manifest.budget);
    if !commit {
        println!(
            "  Note: `{}/` added to .gitignore (use --commit to track in source control)",
            config.export_dir
        );
    }
    Ok(())
}
