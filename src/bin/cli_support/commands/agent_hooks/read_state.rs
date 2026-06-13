use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use synrepo::pipeline::context_metrics;
use synrepo::surface::card::accounting::estimate_tokens_bytes;

use super::{classify, HookClient, HookEvent};

mod command;

use command::{extract_direct_reader_path, path_token_is_safe};

const STATE_FILE: &str = "agent-hook-reads.json";
const LOCK_FILE: &str = "agent-hook-reads.lock";
const MAX_ENTRIES: usize = 256;
const TTL_SECS: u64 = 8 * 60 * 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReadHint {
    pub(super) path: String,
    pub(super) estimated_tokens: usize,
    pub(super) repeated: bool,
}

#[derive(Debug)]
struct ReadObservation {
    rel_path: String,
    metadata: ReadMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReadState {
    #[serde(default)]
    entries: Vec<ReadEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReadEntry {
    path: String,
    size_bytes: u64,
    modified_unix_secs: u64,
    #[serde(default)]
    modified_unix_nanos: u64,
    first_seen_unix_secs: u64,
    last_seen_unix_secs: u64,
    count: u64,
    estimated_tokens: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReadMetadata {
    size_bytes: u64,
    modified_unix_secs: u64,
    modified_unix_nanos: u64,
    estimated_tokens: usize,
}

struct StateLock {
    _file: File,
}

pub(super) fn read_hint_best_effort(
    client: HookClient,
    event: HookEvent,
    input: &Value,
    synrepo_dir: &Path,
) -> Option<ReadHint> {
    if event != HookEvent::PreToolUse {
        return None;
    }
    let repo_root = synrepo_dir.parent()?;
    let raw_path = read_path_from_input(client, input)?;
    let observation = observe_path(repo_root, &raw_path).ok()??;
    let hint = update_state(synrepo_dir, observation, now_secs()).ok()??;
    context_metrics::record_hook_file_read_best_effort(
        synrepo_dir,
        hint.repeated,
        hint.estimated_tokens,
    );
    Some(hint)
}

fn read_path_from_input(client: HookClient, input: &Value) -> Option<String> {
    let tool_name = input.get("tool_name").and_then(Value::as_str)?;
    match (client, tool_name) {
        (HookClient::Claude, "Read") => extract_tool_path(input).map(str::to_string),
        (HookClient::Codex, "Bash") => {
            let command = classify::extract_command(input)?;
            extract_direct_reader_path(command)
        }
        _ => None,
    }
}

fn extract_tool_path(input: &Value) -> Option<&str> {
    let tool_input = input.get("tool_input");
    tool_input
        .and_then(|value| value.get("file_path"))
        .and_then(Value::as_str)
        .or_else(|| {
            tool_input
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
        })
        .or_else(|| input.get("file_path").and_then(Value::as_str))
        .or_else(|| input.get("path").and_then(Value::as_str))
}

fn observe_path(repo_root: &Path, raw_path: &str) -> anyhow::Result<Option<ReadObservation>> {
    if !path_token_is_safe(raw_path) {
        return Ok(None);
    }
    let repo_root = repo_root.canonicalize()?;
    let path = PathBuf::from(raw_path);
    let candidate = if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    };
    let candidate = candidate.canonicalize()?;
    if !candidate.starts_with(&repo_root) {
        return Ok(None);
    }
    let rel_path = candidate
        .strip_prefix(&repo_root)
        .ok()
        .and_then(path_to_storage_string);
    let Some(rel_path) = rel_path else {
        return Ok(None);
    };
    if rel_path == ".synrepo" || rel_path.starts_with(".synrepo/") {
        return Ok(None);
    }
    let metadata = fs::metadata(&candidate)?;
    if !metadata.is_file() {
        return Ok(None);
    }
    // Resolve mtime once: `modified()` can be a syscall, and this runs on the
    // PreToolUse path of every observed read.
    let modified = metadata.modified().ok();
    Ok(Some(ReadObservation {
        rel_path,
        metadata: ReadMetadata {
            size_bytes: metadata.len(),
            modified_unix_secs: metadata_time_secs(modified),
            modified_unix_nanos: metadata_time_nanos(modified),
            estimated_tokens: estimate_tokens_bytes(metadata.len() as usize),
        },
    }))
}

fn path_to_storage_string(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        parts.push(component.as_os_str().to_str()?);
    }
    Some(parts.join("/"))
}

fn metadata_time_secs(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn metadata_time_nanos(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| {
            duration
                .as_secs()
                .saturating_mul(1_000_000_000)
                .saturating_add(duration.subsec_nanos() as u64)
        })
        .unwrap_or(0)
}

fn update_state(
    synrepo_dir: &Path,
    observation: ReadObservation,
    now: u64,
) -> anyhow::Result<Option<ReadHint>> {
    let state_dir = synrepo_dir.join("state");
    fs::create_dir_all(&state_dir)?;
    let Some(_lock) = try_state_lock(&state_dir.join(LOCK_FILE))? else {
        return Ok(None);
    };
    let path = state_dir.join(STATE_FILE);
    let mut state = read_state(&path)?;
    state
        .entries
        .retain(|entry| now.saturating_sub(entry.last_seen_unix_secs) <= TTL_SECS);

    let mut repeated = false;
    // Unchanged == same size and same mtime (secs + nanos). nanos discriminates
    // sub-second edits on fine-grained filesystems; on coarse (1s-resolution)
    // mounts nanos is always 0, so a same-length edit within one second can
    // still read as unchanged. We deliberately do not hash file content here
    // (the state stores only metadata), so this residual is accepted.
    match state
        .entries
        .iter_mut()
        .find(|entry| entry.path == observation.rel_path)
    {
        Some(entry)
            if entry.size_bytes == observation.metadata.size_bytes
                && entry.modified_unix_secs == observation.metadata.modified_unix_secs
                && entry.modified_unix_nanos == observation.metadata.modified_unix_nanos =>
        {
            entry.count += 1;
            entry.last_seen_unix_secs = now;
            entry.estimated_tokens = observation.metadata.estimated_tokens;
            repeated = entry.count > 1;
        }
        Some(entry) => {
            *entry = new_entry(&observation, now);
        }
        None => state.entries.push(new_entry(&observation, now)),
    }

    state
        .entries
        .sort_by_key(|entry| std::cmp::Reverse(entry.last_seen_unix_secs));
    state.entries.truncate(MAX_ENTRIES);
    let bytes = serde_json::to_vec_pretty(&state)?;
    synrepo::util::atomic_write(&path, &bytes)?;

    Ok(Some(ReadHint {
        path: observation.rel_path,
        estimated_tokens: observation.metadata.estimated_tokens,
        repeated,
    }))
}

fn new_entry(observation: &ReadObservation, now: u64) -> ReadEntry {
    ReadEntry {
        path: observation.rel_path.clone(),
        size_bytes: observation.metadata.size_bytes,
        modified_unix_secs: observation.metadata.modified_unix_secs,
        modified_unix_nanos: observation.metadata.modified_unix_nanos,
        first_seen_unix_secs: now,
        last_seen_unix_secs: now,
        count: 1,
        estimated_tokens: observation.metadata.estimated_tokens,
    }
}

fn read_state(path: &Path) -> anyhow::Result<ReadState> {
    if !path.exists() {
        return Ok(ReadState {
            entries: Vec::new(),
        });
    }
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn try_state_lock(path: &Path) -> std::io::Result<Option<StateLock>> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    let file = options.open(path)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(StateLock { _file: file })),
        Err(_) => Ok(None),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
