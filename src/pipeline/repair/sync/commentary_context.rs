//! Compatibility wrapper for commentary prompt context assembly.

use std::path::Path;

use crate::pipeline::explain::context::{
    build_context_text as build_explain_context, CommentaryContextOptions, CommentaryContextTarget,
};
use crate::pipeline::repair::commentary::CommentaryNodeSnapshot;
use crate::structure::graph::GraphReader;

pub(super) fn build_context_text(
    repo_root: &Path,
    graph: &dyn GraphReader,
    snap: &CommentaryNodeSnapshot,
    max_input_tokens: u32,
) -> String {
    let target = CommentaryContextTarget::new(
        snap.content_hash.clone(),
        snap.file.clone(),
        snap.symbol.clone(),
    );
    build_explain_context(
        repo_root,
        graph,
        &target,
        CommentaryContextOptions {
            max_input_tokens,
            ..CommentaryContextOptions::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::{
        config::Config,
        core::ids::NodeId,
        pipeline::{
            explain::context::{
                build_context_text as build_direct_context, CommentaryContextTarget,
            },
            repair::commentary::resolve_commentary_node,
            structural::run_structural_compile,
        },
        store::sqlite::SqliteGraphStore,
    };

    #[test]
    fn repair_wrapper_matches_shared_explain_context() {
        let (repo, graph) = fixture();
        let file = graph.file_by_path("src/main.ts").unwrap().unwrap();
        let snap = resolve_commentary_node(&graph, NodeId::File(file.id))
            .unwrap()
            .unwrap();
        let target = CommentaryContextTarget::new(
            snap.content_hash.clone(),
            snap.file.clone(),
            snap.symbol.clone(),
        );

        let via_repair = build_context_text(repo.path(), &graph, &snap, 20_000);
        let direct = build_direct_context(
            repo.path(),
            &graph,
            &target,
            CommentaryContextOptions {
                max_input_tokens: 20_000,
                ..CommentaryContextOptions::default()
            },
        );

        assert_eq!(via_repair, direct);
        assert!(via_repair.contains("<imports>"));
    }

    fn fixture() -> (TempDir, SqliteGraphStore) {
        let repo = TempDir::new().unwrap();
        fs::create_dir_all(repo.path().join("src")).unwrap();
        fs::write(
            repo.path().join("src/utils.ts"),
            "export function helper() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            repo.path().join("src/main.ts"),
            "import { helper } from './utils';\nexport function main() { return helper(); }\n",
        )
        .unwrap();

        let graph_dir = repo.path().join(".synrepo/graph");
        let mut graph = SqliteGraphStore::open(&graph_dir).unwrap();
        run_structural_compile(repo.path(), &Config::default(), &mut graph).unwrap();
        (repo, graph)
    }
}
