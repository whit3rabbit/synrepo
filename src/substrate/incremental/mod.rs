//! Incremental substrate index maintenance backed by `syntext`, complementing the full `build_index()` path for watch-driven updates.
//!
//! Uses `syntext::changes::Catalogue` for BLAKE3 content-fingerprinting so
//! a `Touched` file (size/mtime changed but bytes identical) produces no
//! index work. `Index::apply_change_batch` accepts pre-loaded content buffers
//! from the catalogue so files are read exactly once. `Index::flush_overlay`
//! persists the committed overlay so changes survive a process restart even
//! when compaction does not run.

mod pending;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::Config;
use pending::{
    collect_pending_actions, manifest_path, matcher_for_repo, syntext_config, PendingAction,
};
use syntext::changes::{Catalogue, ChangeBatch, ChangeKind, DEFAULT_MAX_RETAIN_BYTES};
use syntext::changes::{ChangeRecord, ContentDigest, FileFingerprint};
use syntext::index::Index;
use syntext::IndexError;

/// Name used when acknowledging the lexical-index consumer in the catalogue.
const LEXICAL_CONSUMER: &str = "lexical";

/// How the repo index was updated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexSyncMode {
    /// Applied queued file changes through syntext's overlay and committed them.
    Incremental,
    /// Rebuilt the whole index from scratch.
    Rebuild,
}

/// Result of syncing the repo lexical index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexSyncReport {
    /// Which maintenance path was used.
    pub mode: IndexSyncMode,
    /// Number of files queued as changed.
    pub changed_paths: usize,
    /// Number of files queued as deleted/evicted.
    pub deleted_paths: usize,
    /// Whether the applied changes were flushed durably to disk.
    /// False only when `flush_overlay` returned `Ok(false)` due to an
    /// inline full rebuild (which is itself durable).
    pub durable: bool,
}

/// Internal error that distinguishes a recoverable "needs rebuild" situation
/// or transient lock conflict from a fatal, unrecoverable failure.
enum IncrementalError {
    LockConflict,
    NeedRebuild,
    Fatal(crate::Error),
}

const LOCK_RETRY_SCHEDULE_MS: [u64; 5] = [20, 40, 80, 160, 200];

/// Incrementally update the persisted syntext index from a touched-path set.
/// Falls back to a full rebuild when the incremental state is missing or unusable.
///
/// Uses `syntext::changes::Catalogue` for BLAKE3 fingerprinting so identical
/// bytes produce no index work. Changes are made durable via `flush_overlay`.
pub fn sync_index_incremental(
    config: &Config,
    repo_root: &Path,
    touched_paths: &[PathBuf],
) -> crate::Result<IndexSyncReport> {
    let mut attempt = 0;
    loop {
        match sync_index_incremental_inner(config, repo_root, touched_paths) {
            Ok(report) => return Ok(report),
            Err(IncrementalError::LockConflict) => {
                if attempt < LOCK_RETRY_SCHEDULE_MS.len() {
                    std::thread::sleep(std::time::Duration::from_millis(
                        LOCK_RETRY_SCHEDULE_MS[attempt],
                    ));
                    attempt += 1;
                    continue;
                }
                return Err(crate::Error::Other(anyhow::anyhow!(
                    "substrate index at {} is locked by another process",
                    Config::synrepo_dir(repo_root).join("index").display()
                )));
            }
            Err(IncrementalError::NeedRebuild) => {
                return rebuild_index(config, repo_root);
            }
            Err(IncrementalError::Fatal(err)) => return Err(err),
        }
    }
}

fn sync_index_incremental_inner(
    config: &Config,
    repo_root: &Path,
    touched_paths: &[PathBuf],
) -> Result<IndexSyncReport, IncrementalError> {
    let redaction_matcher = matcher_for_repo(config, repo_root).map_err(IncrementalError::Fatal)?;
    let pending = collect_pending_actions(config, repo_root, touched_paths, &redaction_matcher)
        .map_err(IncrementalError::Fatal)?;
    if pending.is_empty() {
        if manifest_path(config, repo_root).exists() {
            return Ok(IndexSyncReport {
                mode: IndexSyncMode::Incremental,
                changed_paths: 0,
                deleted_paths: 0,
                durable: true,
            });
        }
        return Err(IncrementalError::NeedRebuild);
    }

    let changed_paths = pending
        .values()
        .filter(|action| matches!(action, PendingAction::Change))
        .count();
    let deleted_paths = pending
        .values()
        .filter(|action| matches!(action, PendingAction::Delete))
        .count();

    if !manifest_path(config, repo_root).exists() {
        return Err(IncrementalError::NeedRebuild);
    }

    let syntext_cfg = syntext_config(config, repo_root);
    let index_dir = syntext_cfg.index_dir.clone();

    // Open the catalogue for fingerprint-based change detection. A missing
    // or corrupt catalogue is non-fatal: we fall back to treating every
    // pending path as changed (the old behaviour).
    let catalogue_result = Catalogue::open_or_create(&index_dir);

    // Build the candidate path list for catalogue observation (absolute paths).
    let candidate_paths: Vec<PathBuf> = pending.keys().map(|rel| repo_root.join(rel)).collect();

    // Observe paths and produce a generation-tagged ChangeBatch.
    // If the catalogue load failed, synthesise a batch from the pending map.
    let read_epoch = SystemTime::now();
    let (mut batch, mut catalogue_opt) = match catalogue_result {
        Ok(mut catalogue) => {
            let batch = catalogue.observe_paths(repo_root, &candidate_paths, read_epoch);
            (batch, Some(catalogue))
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "substrate catalogue unavailable; proceeding without fingerprint check"
            );
            let batch = synthesise_batch_from_pending(&pending, repo_root);
            (batch, None)
        }
    };

    // The catalogue only emits `Deleted` for paths it already tracked.
    // A file that was never synced through the catalogue (e.g. first run after
    // bootstrap, or a redacted/skipped file) will produce no catalogue record,
    // but syntext's index may still hold stale content for it. Supplement the
    // batch with explicit `Deleted` records for any `PendingAction::Delete` path
    // not already covered by the catalogue's output.
    {
        let covered: std::collections::HashSet<PathBuf> = batch
            .records
            .iter()
            .filter(|r| r.kind.is_deleted())
            .map(|r| r.path.clone())
            .collect();
        let extra: Vec<ChangeRecord> = pending
            .iter()
            .filter(|(rel, action)| {
                matches!(action, PendingAction::Delete) && !covered.contains(*rel)
            })
            .map(|(rel, _)| ChangeRecord {
                path: rel.clone(),
                kind: ChangeKind::Deleted,
                current_fingerprint: None,
                previous_fingerprint: None,
                content: None,
            })
            .collect();
        batch.records.extend(extra);
    }

    // If every path is Unchanged or Touched (bytes identical), skip index work.
    let has_content_work = batch
        .records
        .iter()
        .any(|r| r.kind.is_content_changed() || r.kind.is_deleted());

    if !has_content_work {
        if let Some(ref mut cat) = catalogue_opt {
            cat.acknowledge(LEXICAL_CONSUMER, batch.generation);
            if let Err(err) = cat.save() {
                tracing::warn!(error = %err, "substrate catalogue save failed");
            }
        }
        return Ok(IndexSyncReport {
            mode: IndexSyncMode::Incremental,
            changed_paths: 0,
            deleted_paths: 0,
            durable: true,
        });
    }

    // --- Incremental apply, scoped so the Index handle is dropped before any
    // rebuild fallback. If we hold the handle open while rebuild_index tries to
    // acquire an exclusive lock we deadlock ourselves.
    let apply_result: Result<(u64, bool), IncrementalError> = {
        match Index::open(syntext_cfg) {
            Err(IndexError::LockConflict(_)) => Err(IncrementalError::LockConflict),
            Err(IndexError::CorruptIndex(_) | IndexError::Io(_)) => {
                Err(IncrementalError::NeedRebuild)
            }
            Err(err) => Err(IncrementalError::Fatal(map_index_error(err))),
            Ok(index) => {
                // Apply the change batch, using pre-loaded content buffers from
                // the catalogue to avoid re-reading files.
                match index.apply_change_batch(&batch) {
                    Err(IndexError::LockConflict(_)) => Err(IncrementalError::LockConflict),
                    Err(
                        IndexError::OverlayFull { .. }
                        | IndexError::CorruptIndex(_)
                        | IndexError::Io(_),
                    ) => Err(IncrementalError::NeedRebuild),
                    Err(err) => Err(IncrementalError::Fatal(map_index_error(err))),
                    Ok(applied_gen) => {
                        // Flush the committed overlay to disk so changes survive
                        // a process restart even when compaction does not run.
                        let durable = match index.flush_overlay() {
                            Ok(flushed) => flushed,
                            Err(IndexError::LockConflict(_)) => {
                                // A competing reader holds the lock. The in-memory
                                // overlay is committed; the flush will be retried on
                                // the next reconcile pass. Not a rebuild trigger.
                                tracing::warn!(
                                    "substrate flush_overlay skipped: lock conflict (will retry)"
                                );
                                false
                            }
                            Err(err) => return Err(IncrementalError::Fatal(map_index_error(err))),
                        };
                        Ok((applied_gen, durable))
                    }
                }
            }
        }
    }; // Index handle is dropped here.

    let (applied_gen, durable) = apply_result?;
    if durable {
        if let Some(ref mut cat) = catalogue_opt {
            cat.acknowledge(LEXICAL_CONSUMER, applied_gen);
            if let Err(err) = cat.save() {
                tracing::warn!(error = %err, "substrate catalogue save failed");
            }
        }
    }
    Ok(IndexSyncReport {
        mode: IndexSyncMode::Incremental,
        changed_paths,
        deleted_paths,
        durable,
    })
}

/// Synthesise a `ChangeBatch` from the pending action map when the catalogue
/// is unavailable. Every pending path is treated as an `Added` change so that
/// all content is applied conservatively.
fn synthesise_batch_from_pending(
    pending: &BTreeMap<PathBuf, PendingAction>,
    repo_root: &Path,
) -> ChangeBatch {
    use std::sync::Arc;

    let records = pending
        .iter()
        .map(|(rel, action)| {
            let kind = match action {
                PendingAction::Change => ChangeKind::Added,
                PendingAction::Delete => ChangeKind::Deleted,
            };
            // Pre-read small files so apply_change_batch can skip a second disk read.
            let (content, current_fingerprint) = if matches!(action, PendingAction::Change) {
                let abs = repo_root.join(rel);
                match fs::read(&abs) {
                    Ok(bytes) if bytes.len() <= DEFAULT_MAX_RETAIN_BYTES => {
                        let digest = ContentDigest::from_bytes(&bytes);
                        let size = bytes.len() as u64;
                        let fp = FileFingerprint::new(digest, size, 0, 0);
                        (Some(Arc::from(bytes.as_slice())), Some(fp))
                    }
                    _ => (None, None),
                }
            } else {
                (None, None)
            };
            ChangeRecord {
                path: rel.clone(),
                kind,
                current_fingerprint,
                previous_fingerprint: None,
                content,
            }
        })
        .collect();
    ChangeBatch::new(1, records, false)
}

fn rebuild_index(config: &Config, repo_root: &Path) -> crate::Result<IndexSyncReport> {
    let report = super::build_index(config, repo_root)?;
    Ok(IndexSyncReport {
        mode: IndexSyncMode::Rebuild,
        changed_paths: report.indexed_files,
        deleted_paths: 0,
        durable: true,
    })
}

pub(crate) fn should_rebuild(error: &syntext::IndexError) -> bool {
    matches!(error, IndexError::CorruptIndex(_) | IndexError::Io(_))
}

fn map_index_error(error: syntext::IndexError) -> crate::Error {
    match error {
        IndexError::CorruptIndex(message) => crate::Error::Other(anyhow::anyhow!(
            "substrate index is unusable: {message}. Re-run `synrepo init` to rebuild it."
        )),
        IndexError::LockConflict(path) => crate::Error::Other(anyhow::anyhow!(
            "substrate index at {} is locked by another process",
            path.display()
        )),
        other => crate::Error::Other(anyhow::anyhow!(
            "unable to update substrate index incrementally: {other}"
        )),
    }
}
