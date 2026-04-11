use crate::core::provenance::{Provenance, SourceRef};

/// Build a `Provenance` record for a structural-pipeline row.
pub(super) fn make_provenance(
    pass: &str,
    revision: &str,
    path: &str,
    content_hash: &str,
) -> Provenance {
    Provenance::structural(
        pass,
        revision,
        vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: content_hash.to_string(),
        }],
    )
}
