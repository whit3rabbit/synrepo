use std::path::{Path, PathBuf};

use synrepo::{
    config::Config,
    pipeline::explain::docs::{
        commentary_doc_paths, import_commentary_doc, list_commentary_docs,
        reconcile_commentary_docs, search_commentary_docs, sync_commentary_index,
        CommentaryDocImportStatus,
    },
    store::{overlay::SqliteOverlayStore, sqlite::SqliteGraphStore},
    structure::graph::with_graph_read_snapshot,
};

use crate::cli_support::cli_args::DocsCommand;

/// Run the `synrepo docs` command group.
pub(crate) fn docs(repo_root: &Path, command: DocsCommand) -> anyhow::Result<()> {
    match command {
        DocsCommand::Export => print!("{}", docs_export_output(repo_root)?),
        DocsCommand::List => print!("{}", docs_list_output(repo_root)?),
        DocsCommand::Search { query, limit } => {
            print!("{}", docs_search_output(repo_root, &query, limit)?)
        }
        DocsCommand::Import { all, path } => {
            print!("{}", docs_import_output(repo_root, all, path.as_deref())?)
        }
    }
    Ok(())
}

pub(crate) fn docs_export_output(repo_root: &Path) -> anyhow::Result<String> {
    let synrepo_dir = ensure_initialized(repo_root)?;
    let graph = open_graph(&synrepo_dir)?;
    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).ok();
    let touched = with_graph_read_snapshot(&graph, |graph| {
        reconcile_commentary_docs(&synrepo_dir, graph, overlay.as_ref())
    })?;
    let index = sync_commentary_index(&synrepo_dir, &touched)?;
    let total = list_commentary_docs(&synrepo_dir)?.len();

    Ok(format!(
        "Explain docs exported: {total} docs, {} changed\n  Directory: {}\n  Index: {:?} ({} touched)\n",
        touched.len(),
        synrepo_dir.join("explain-docs").display(),
        index.mode,
        index.touched_paths,
    ))
}

pub(crate) fn docs_list_output(repo_root: &Path) -> anyhow::Result<String> {
    let synrepo_dir = ensure_initialized(repo_root)?;
    let docs = list_commentary_docs(&synrepo_dir)?;
    let mut file_count = 0usize;
    let mut symbol_count = 0usize;
    for doc in &docs {
        match doc.node_kind.as_str() {
            "file" => file_count += 1,
            "symbol" => symbol_count += 1,
            _ => {}
        }
    }

    let mut out = format!(
        "Explain docs: {} total ({} files, {} symbols)\n",
        docs.len(),
        file_count,
        symbol_count
    );
    for doc in docs {
        out.push_str(&format!(
            "{} [{} {}] {}\n",
            display_path(repo_root, &doc.path),
            doc.node_kind,
            doc.commentary_state,
            doc.source_path,
        ));
    }
    Ok(out)
}

pub(crate) fn docs_search_output(
    repo_root: &Path,
    query: &str,
    limit: u32,
) -> anyhow::Result<String> {
    let synrepo_dir = ensure_initialized(repo_root)?;
    let graph = open_graph(&synrepo_dir)?;
    let overlay = SqliteOverlayStore::open_existing(&synrepo_dir.join("overlay")).ok();
    let hits = with_graph_read_snapshot(&graph, |graph| {
        search_commentary_docs(&synrepo_dir, graph, overlay.as_ref(), query, limit as usize)
    })?;

    let mut out = format!("Explain docs search: {} matches\n", hits.len());
    for hit in hits {
        out.push_str(&format!(
            "{}:{} [{}] {}\n",
            hit.path, hit.line, hit.commentary_state, hit.content
        ));
    }
    Ok(out)
}

pub(crate) fn docs_import_output(
    repo_root: &Path,
    all: bool,
    path: Option<&Path>,
) -> anyhow::Result<String> {
    let synrepo_dir = ensure_initialized(repo_root)?;
    if !all && path.is_none() {
        anyhow::bail!("docs import requires `--all` or a doc path");
    }
    let graph = open_graph(&synrepo_dir)?;
    let mut overlay = SqliteOverlayStore::open(&synrepo_dir.join("overlay"))?;
    let paths = if all {
        commentary_doc_paths(&synrepo_dir)?
    } else {
        vec![resolve_cli_path(repo_root, path.expect("checked above"))]
    };

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut out = String::new();
    for path in paths {
        let outcome = with_graph_read_snapshot(&graph, |graph| {
            import_commentary_doc(graph, &mut overlay, &path)
        })?;
        match outcome.status {
            CommentaryDocImportStatus::Imported => imported += 1,
            CommentaryDocImportStatus::Skipped => skipped += 1,
        }
        out.push_str(&format!(
            "{}: {}",
            display_path(repo_root, &outcome.path),
            outcome.status.as_str()
        ));
        if let Some(reason) = outcome.reason {
            out.push_str(&format!(" ({reason})"));
        }
        out.push('\n');
    }
    out.insert_str(
        0,
        &format!("Explain docs import: {imported} imported, {skipped} skipped\n"),
    );
    Ok(out)
}

fn ensure_initialized(repo_root: &Path) -> anyhow::Result<PathBuf> {
    Config::load(repo_root).map_err(|err| {
        anyhow::anyhow!("docs: not initialized, run `synrepo init` first ({err})")
    })?;
    Ok(Config::synrepo_dir(repo_root))
}

fn open_graph(synrepo_dir: &Path) -> anyhow::Result<SqliteGraphStore> {
    SqliteGraphStore::open_existing(&synrepo_dir.join("graph"))
        .map_err(|err| anyhow::anyhow!("docs: graph store unavailable ({err})"))
}

fn resolve_cli_path(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn display_path(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
