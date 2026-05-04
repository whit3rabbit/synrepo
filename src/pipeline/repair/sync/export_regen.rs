//! Export regeneration repair handler.

use anyhow::anyhow;

use super::handlers::ActionContext;

/// Re-run export generation.
pub(super) fn regenerate_exports(
    context: &ActionContext<'_>,
    actions_taken: &mut Vec<String>,
) -> crate::Result<()> {
    use crate::pipeline::export::{load_manifest, write_exports, ExportFormat};
    use crate::surface::card::Budget;

    let existing = load_manifest(context.repo_root, context.config);
    let format = existing
        .as_ref()
        .map(|m| m.format)
        .unwrap_or(ExportFormat::Markdown);
    let budget = existing
        .as_ref()
        .and_then(|m| match m.budget.as_str() {
            "deep" => Some(Budget::Deep),
            "normal" => Some(Budget::Normal),
            _ => None,
        })
        .unwrap_or(Budget::Normal);

    write_exports(
        context.repo_root,
        context.synrepo_dir,
        context.config,
        format,
        budget,
        false,
    )
    .map_err(|e| anyhow!("{e}"))?;

    actions_taken.push(format!(
        "regenerated export directory (format={}, budget={})",
        format.as_str(),
        match budget {
            Budget::Tiny => "tiny",
            Budget::Normal => "normal",
            Budget::Deep => "deep",
        }
    ));
    Ok(())
}
