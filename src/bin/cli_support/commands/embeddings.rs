use std::path::Path;

use synrepo::{
    config::Config,
    pipeline::writer::{acquire_write_admission, map_lock_error},
    store::sqlite::SqliteGraphStore,
    substrate::embedding::{
        build_embedding_index_with_progress, is_available, EmbeddingBuildEvent,
        EmbeddingBuildSummary,
    },
};

use crate::cli_support::cli_args::EmbeddingsCommand;

pub(crate) fn embeddings(repo_root: &Path, command: EmbeddingsCommand) -> anyhow::Result<()> {
    match command {
        EmbeddingsCommand::Build { json } => match build_output(repo_root, json, !json) {
            Ok(output) => {
                print!("{output}");
                Ok(())
            }
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "status": "error",
                            "error": error.to_string(),
                        }))?
                    );
                }
                Err(error)
            }
        },
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn embeddings_build_output(repo_root: &Path, json: bool) -> anyhow::Result<String> {
    build_output(repo_root, json, false)
}

pub(crate) fn embeddings_build_human(repo_root: &Path) -> anyhow::Result<()> {
    print!("{}", build_output(repo_root, false, true)?);
    Ok(())
}

fn build_output(repo_root: &Path, json: bool, stream_progress: bool) -> anyhow::Result<String> {
    if !is_available() {
        anyhow::bail!(
            "embeddings build: this binary was not built with `semantic-triage`; rebuild with `cargo build --features semantic-triage`"
        );
    }

    let config = Config::load(repo_root).map_err(|error| {
        anyhow::anyhow!("embeddings build: not initialized, run `synrepo init` first ({error})")
    })?;
    if !config.enable_semantic_triage {
        anyhow::bail!(
            "embeddings build: embeddings are disabled; enable them with `synrepo dashboard` (T) or set enable_semantic_triage = true"
        );
    }

    let synrepo_dir = Config::synrepo_dir(repo_root);
    let _lock = acquire_write_admission(&synrepo_dir, "embeddings build")
        .map_err(|error| map_lock_error("embeddings build", error))?;
    let graph = SqliteGraphStore::open(&synrepo_dir.join("graph")).map_err(|error| {
        anyhow::anyhow!("embeddings build: could not open graph store ({error})")
    })?;

    let mut progress_cb = |event: EmbeddingBuildEvent| {
        if let Some(line) = progress_line(&event) {
            eprintln!("{line}");
        }
    };
    let progress: Option<&mut dyn FnMut(EmbeddingBuildEvent)> = if stream_progress {
        Some(&mut progress_cb)
    } else {
        None
    };

    let summary =
        build_embedding_index_with_progress(&graph, &config, &synrepo_dir, progress, None)
            .map_err(|error| anyhow::anyhow!("embeddings build failed: {error}"))?;
    render_summary(&summary, json)
}

fn render_summary(summary: &EmbeddingBuildSummary, json: bool) -> anyhow::Result<String> {
    if json {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "completed",
                "summary": summary,
            }))?
        ));
    }
    Ok(format!(
        "Embedding index built: {} chunks\n  provider: {}\n  model: {} ({}d)\n  index: {}\n",
        summary.chunks,
        summary.provider,
        summary.model,
        summary.dim,
        summary.index_path.display()
    ))
}

fn progress_line(event: &EmbeddingBuildEvent) -> Option<String> {
    match event {
        EmbeddingBuildEvent::ResolvingModel {
            provider,
            model,
            dim,
        } => Some(format!("embeddings: resolving {provider}/{model} ({dim}d)")),
        EmbeddingBuildEvent::ModelReady {
            provider,
            model,
            downloaded,
            ..
        } => Some(format!(
            "embeddings: {provider}/{model} ready{}",
            if *downloaded { " (downloaded)" } else { "" }
        )),
        EmbeddingBuildEvent::InitializingBackend => {
            Some("embeddings: initializing backend".to_string())
        }
        EmbeddingBuildEvent::PreflightStarted => {
            Some("embeddings: running provider preflight".to_string())
        }
        EmbeddingBuildEvent::PreflightFinished => {
            Some("embeddings: provider preflight ok".to_string())
        }
        EmbeddingBuildEvent::ExtractingChunks => {
            Some("embeddings: extracting graph chunks".to_string())
        }
        EmbeddingBuildEvent::ChunksReady { chunks } => {
            Some(format!("embeddings: {chunks} chunks ready"))
        }
        EmbeddingBuildEvent::BatchFinished { current, total } => {
            if *current == *total || *current % 25 == 0 {
                Some(format!("embeddings: embedded {current}/{total} chunks"))
            } else {
                None
            }
        }
        EmbeddingBuildEvent::SavingIndex { path } => {
            Some(format!("embeddings: saving {}", path.display()))
        }
        EmbeddingBuildEvent::Finished { chunks, path, .. } => Some(format!(
            "embeddings: complete ({chunks} chunks, {})",
            path.display()
        )),
    }
}
