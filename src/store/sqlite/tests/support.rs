use crate::core::provenance::{CreatedBy, Provenance, SourceRef};
use time::OffsetDateTime;

pub(super) fn sample_provenance(pass: &str, path: &str) -> Provenance {
    Provenance {
        created_at: OffsetDateTime::UNIX_EPOCH,
        source_revision: "deadbeef".to_string(),
        created_by: CreatedBy::StructuralPipeline,
        pass: pass.to_string(),
        source_artifacts: vec![SourceRef {
            file_id: None,
            path: path.to_string(),
            content_hash: "hash".to_string(),
        }],
    }
}
