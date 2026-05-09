use std::path::PathBuf;

use serde_json::json;

pub(super) struct PlannedFile {
    pub(super) root_id: String,
    pub(super) path: String,
    pub(super) abs_path: PathBuf,
    pub(super) original: Vec<u8>,
    pub(super) next: Vec<u8>,
    pub(super) new_hash: String,
    pub(super) edit_count: u64,
}

pub(super) struct WriteOutcome {
    pub(super) file_results: Vec<serde_json::Value>,
    pub(super) touched: Vec<String>,
    pub(super) applied: bool,
}

pub(super) fn write_planned_files(planned: &[PlannedFile]) -> WriteOutcome {
    let mut file_results = Vec::new();
    let mut touched = Vec::new();
    let mut written = Vec::<&PlannedFile>::new();

    for (idx, file) in planned.iter().enumerate() {
        if let Err(error) = crate::util::atomic_write(&file.abs_path, &file.next) {
            let rollback = rollback_written(&written);
            file_results.extend(written.iter().map(|written| {
                json!({
                    "path": written.path,
                    "root_id": written.root_id,
                    "status": if rollback.ok { "rolled_back" } else { "rollback_failed" },
                    "new_content_hash": written.new_hash,
                })
            }));
            file_results.push(json!({
                "path": file.path,
                "root_id": file.root_id,
                "status": "rejected",
                "error": error.to_string(),
            }));
            file_results.extend(planned.iter().skip(idx + 1).map(|remaining| {
                json!({
                    "path": remaining.path,
                    "root_id": remaining.root_id,
                    "status": "not_applied",
                    "reason": "cross_file_atomic_write_failed",
                })
            }));
            if !rollback.ok {
                file_results.push(json!({
                    "path": rollback.failed_path,
                    "status": "rollback_failed",
                    "error": rollback.error,
                }));
            }
            return WriteOutcome {
                file_results,
                touched: Vec::new(),
                applied: false,
            };
        }
        touched.push(file.path.clone());
        written.push(file);
    }

    file_results.extend(planned.iter().map(|file| {
        json!({
            "path": file.path,
            "root_id": file.root_id,
            "status": "applied",
            "new_content_hash": file.new_hash,
        })
    }));
    WriteOutcome {
        file_results,
        touched,
        applied: true,
    }
}

struct RollbackOutcome {
    ok: bool,
    failed_path: Option<String>,
    error: Option<String>,
}

fn rollback_written(written: &[&PlannedFile]) -> RollbackOutcome {
    for file in written.iter().rev() {
        if let Err(error) = crate::util::atomic_write(&file.abs_path, &file.original) {
            return RollbackOutcome {
                ok: false,
                failed_path: Some(file.path.clone()),
                error: Some(error.to_string()),
            };
        }
    }
    RollbackOutcome {
        ok: true,
        failed_path: None,
        error: None,
    }
}
