//! The lexical substrate layer.
//! 
//! Wraps `syntext` to provide deterministic n-gram lexical retrieval.

use std::path::Path;
use syntext::{Config as SyntextConfig, SearchOptions};
use syntext::index::Index;

/// Builds (or configures for building) the syntext index.
pub fn build_index(config: &crate::config::Config, repo_root: &Path) -> crate::Result<()> {
    // Map synrepo's config to syntext's config.
    let syntext_config = SyntextConfig {
        index_dir: crate::config::Config::synrepo_dir(repo_root).join("index"),
        repo_root: repo_root.to_path_buf(),
        max_file_size: config.max_file_size_bytes,
        ..SyntextConfig::default()
    };

    // syntext::index::Index::build takes ownership of Config and returns a Result.
    Index::build(syntext_config)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("syntext build error: {:?}", e)))?;
    
    Ok(())
}

/// Executes a search query via the syntext index.
pub fn search(config: &crate::config::Config, repo_root: &Path, query: &str) -> crate::Result<Vec<syntext::SearchMatch>> {
    let syntext_config = SyntextConfig {
        index_dir: crate::config::Config::synrepo_dir(repo_root).join("index"),
        repo_root: repo_root.to_path_buf(),
        max_file_size: config.max_file_size_bytes,
        ..SyntextConfig::default()
    };

    let index = Index::open(syntext_config)
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("syntext open error: {:?}", e)))?;
        
    let results = index.search(query, &SearchOptions::default())
        .map_err(|e| crate::Error::Other(anyhow::anyhow!("syntext search error: {:?}", e)))?;
        
    Ok(results)
}
