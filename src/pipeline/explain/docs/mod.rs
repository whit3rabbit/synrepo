//! Materialized explaind commentary docs and their dedicated syntext index.

mod corpus;
mod edit;
mod index;
mod maintenance;
mod search;

pub use corpus::{
    commentary_doc_relative_path, delete_commentary_doc, docs_root, index_dir,
    parse_commentary_doc, parse_commentary_doc_header, reconcile_commentary_docs,
    repo_relative_doc_path, upsert_commentary_doc, CommentaryDocHeader,
    CommentaryDocSymbolMetadata,
};
pub use edit::{
    commentary_doc_paths, import_commentary_doc, list_commentary_docs, CommentaryDocImportOutcome,
    CommentaryDocImportStatus, CommentaryDocListItem,
};
pub use index::{
    search_commentary_index, sync_commentary_index, CommentaryIndexSyncMode,
    CommentaryIndexSyncSummary,
};
pub use maintenance::{
    clean_commentary_docs, export_commentary_docs, CommentaryDocsCleanSummary,
    CommentaryDocsExportOptions, CommentaryDocsExportSummary,
};
pub use search::{search_commentary_docs, CommentaryDocHit};
