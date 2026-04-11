use std::path::Path;

use synrepo::{
    config::{Config, Mode},
    store::compatibility::StoreId,
};

use super::graph::{check_store_ready, graph_query_output, graph_stats_output, node_output};

pub(crate) fn init(repo_root: &Path, requested_mode: Option<Mode>) -> anyhow::Result<()> {
    let report = synrepo::bootstrap::bootstrap(repo_root, requested_mode)?;
    print!("{}", report.render());
    Ok(())
}

pub(crate) fn search(repo_root: &Path, query: &str) -> anyhow::Result<()> {
    let config = Config::load(repo_root)?;
    let synrepo_dir = Config::synrepo_dir(repo_root);
    check_store_ready(&synrepo_dir, &config, StoreId::Index)?;

    let matches = synrepo::substrate::search(&config, repo_root, query)?;
    for search_match in &matches {
        println!(
            "{}:{}: {}",
            search_match.path.display(),
            search_match.line_number,
            String::from_utf8_lossy(&search_match.line_content).trim_end()
        );
    }

    println!("Found {} matches.", matches.len());
    Ok(())
}

pub(crate) fn graph_query(repo_root: &Path, query: &str) -> anyhow::Result<()> {
    println!("{}", graph_query_output(repo_root, query)?);
    Ok(())
}

pub(crate) fn graph_stats(repo_root: &Path) -> anyhow::Result<()> {
    println!("{}", graph_stats_output(repo_root)?);
    Ok(())
}

pub(crate) fn node(repo_root: &Path, id: &str) -> anyhow::Result<()> {
    println!("{}", node_output(repo_root, id)?);
    Ok(())
}
