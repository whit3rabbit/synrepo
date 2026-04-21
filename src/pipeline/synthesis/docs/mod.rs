//! Materialized synthesized commentary docs and their dedicated syntext index.

mod corpus;
mod index;
mod search;

pub use corpus::{
    commentary_doc_relative_path, delete_commentary_doc, docs_root, index_dir,
    reconcile_commentary_docs, repo_relative_doc_path, upsert_commentary_doc,
    CommentaryDocSymbolMetadata,
};
pub use index::{search_commentary_index, sync_commentary_index};
pub use search::{search_commentary_docs, CommentaryDocHit};
