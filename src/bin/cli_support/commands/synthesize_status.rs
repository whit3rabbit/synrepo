use std::fmt::Write as _;
use std::path::Path;

use synrepo::pipeline::synthesis::{
    build_synthesis_preview, SynthesisPreview, SynthesisPreviewGroup,
};

pub(crate) fn synthesize_status(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<()> {
    print!("{}", synthesize_status_output(repo_root, paths, changed)?);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn synthesize_status_output(
    repo_root: &Path,
    paths: Vec<String>,
    changed: bool,
) -> anyhow::Result<String> {
    let preview = build_synthesis_preview(repo_root, paths, changed)?;

    let mut out = String::new();
    writeln!(out, "Synthesis status:").unwrap();
    writeln!(out, "  scope: {}", preview.scope_label).unwrap();
    writeln!(out, "  provider: {}", preview.provider_label).unwrap();
    writeln!(
        out,
        "  api calls if you run now: {}",
        preview.api_status_line
    )
    .unwrap();
    writeln!(
        out,
        "  output files: overlay-backed commentary under `.synrepo/`, markdown docs under `.synrepo/synthesis-docs/`, searchable index under `.synrepo/synthesis-index/`"
    )
    .unwrap();
    writeln!(
        out,
        "  overlay freshness (whole repo): {}",
        preview.overlay_freshness_line
    )
    .unwrap();

    writeln!(out, "  queued if you run now:").unwrap();
    writeln!(
        out,
        "    stale commentary to refresh: {}",
        preview.refresh.total_count
    )
    .unwrap();
    writeln!(
        out,
        "    files missing commentary: {}",
        preview.file_seeds.total_count
    )
    .unwrap();
    writeln!(
        out,
        "    symbols missing commentary: {}",
        preview.symbol_seeds.total_count
    )
    .unwrap();
    writeln!(
        out,
        "    max targets in this snapshot: {}",
        preview.max_target_count
    )
    .unwrap();
    write_samples(&mut out, &preview)?;
    writeln!(out, "  summary: {}", preview.summary_line).unwrap();

    Ok(out)
}

fn write_samples(out: &mut String, preview: &SynthesisPreview) -> anyhow::Result<()> {
    if preview.max_target_count == 0 {
        return Ok(());
    }

    writeln!(
        out,
        "  sample pending targets (first {} per group):",
        preview.sample_limit_per_group
    )?;
    write_sample_group(out, &preview.refresh)?;
    write_sample_group(out, &preview.file_seeds)?;
    write_sample_group(out, &preview.symbol_seeds)?;
    Ok(())
}

fn write_sample_group(out: &mut String, group: &SynthesisPreviewGroup) -> anyhow::Result<()> {
    if group.total_count == 0 {
        return Ok(());
    }

    writeln!(out, "    {}:", group.label)?;
    for item in &group.items {
        writeln!(out, "      {item}")?;
    }
    if group.remaining_count > 0 {
        writeln!(out, "      … and {} more", group.remaining_count)?;
    }
    Ok(())
}
