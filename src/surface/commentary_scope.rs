use std::path::{Path, PathBuf};

use crate::core::ids::{FileNodeId, NodeId};
use crate::pipeline::repair::load_commentary_work_plan;
use crate::surface::card::{compiler::GraphCardCompiler, CardCompiler};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommentaryRefreshScope {
    Target,
    File,
    Directory,
    Stale,
}

impl CommentaryRefreshScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::File => "file",
            Self::Directory => "directory",
            Self::Stale => "stale",
        }
    }
}

pub(crate) fn resolve_refresh_nodes(
    compiler: &GraphCardCompiler,
    synrepo_dir: &Path,
    scope: CommentaryRefreshScope,
    target: Option<&str>,
) -> anyhow::Result<Vec<NodeId>> {
    match scope {
        CommentaryRefreshScope::Target => {
            let target = required_target(target)?;
            let node_id = compiler
                .resolve_target(target)?
                .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;
            Ok(vec![node_id])
        }
        CommentaryRefreshScope::File => file_scope_nodes(compiler, required_target(target)?),
        CommentaryRefreshScope::Directory => {
            directory_scope_nodes(synrepo_dir, required_target(target)?)
        }
        CommentaryRefreshScope::Stale => stale_scope_nodes(synrepo_dir),
    }
}

fn required_target(target: Option<&str>) -> anyhow::Result<&str> {
    target
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .ok_or_else(|| anyhow::anyhow!("target is required for this scope"))
}

fn file_scope_nodes(compiler: &GraphCardCompiler, target: &str) -> anyhow::Result<Vec<NodeId>> {
    let node_id = compiler
        .resolve_target(target)?
        .ok_or_else(|| anyhow::anyhow!("target not found: {target}"))?;
    let file_id = match node_id {
        NodeId::File(file_id) => file_id,
        NodeId::Symbol(sym_id) => compiler
            .reader()
            .get_symbol(sym_id)?
            .map(|symbol| symbol.file_id)
            .ok_or_else(|| anyhow::anyhow!("symbol not found: {target}"))?,
        NodeId::Concept(_) => anyhow::bail!("file scope requires a file or symbol target"),
    };
    file_and_symbol_nodes(compiler, file_id)
}

fn file_and_symbol_nodes(
    compiler: &GraphCardCompiler,
    file_id: FileNodeId,
) -> anyhow::Result<Vec<NodeId>> {
    let mut nodes = vec![NodeId::File(file_id)];
    for symbol in compiler.reader().symbols_for_file(file_id)? {
        nodes.push(NodeId::Symbol(symbol.id));
    }
    Ok(nodes)
}

fn directory_scope_nodes(synrepo_dir: &Path, target: &str) -> anyhow::Result<Vec<NodeId>> {
    let scope = [PathBuf::from(target)];
    let plan = load_commentary_work_plan(synrepo_dir, Some(&scope))?;
    Ok(plan
        .refresh
        .into_iter()
        .chain(plan.file_seeds)
        .chain(plan.symbol_seed_candidates)
        .map(|item| item.node_id)
        .collect())
}

fn stale_scope_nodes(synrepo_dir: &Path) -> anyhow::Result<Vec<NodeId>> {
    let plan = load_commentary_work_plan(synrepo_dir, None)?;
    Ok(plan.refresh.into_iter().map(|item| item.node_id).collect())
}
