use anyhow::{Context, Result};
use std::path::PathBuf;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::model::{HistoryEntry, ResponseMeta, ResponseRecord, Workspace};
use crate::session::StoredSession;

fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("could not determine data directory")?
        .join("postui");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Where binary response bodies are streamed while a session runs (see `http.rs`).
pub fn bodies_dir() -> Result<PathBuf> {
    let dir = data_dir()?.join("bodies");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Remove streamed bodies from earlier runs (or this one, on quit). Nothing refers to
/// them across sessions.
pub fn clear_bodies_dir() {
    if let Ok(dir) = data_dir().map(|d| d.join("bodies")) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn workspace_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("workspace.json"))
}

pub fn load() -> Result<Workspace> {
    let path = workspace_path()?;
    if !path.exists() {
        return Ok(Workspace::default());
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let workspace: Workspace = serde_json::from_str(&contents).unwrap_or_else(|e| {
        crate::logger::log(
            "workspace load",
            &format!(
                "{} is corrupt ({e}) — starting with an empty workspace to avoid losing the file; \
                 the original is left on disk, inspect/fix it manually",
                path.display()
            ),
        );
        Workspace::default()
    });
    Ok(workspace)
}

pub fn save(workspace: &Workspace) -> Result<()> {
    let path = workspace_path()?;
    let contents =
        serde_json::to_string_pretty(workspace).context("failed to serialize workspace")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Read a side file (drafts, history). Missing → default; corrupt → logged and default,
/// leaving the file on disk for inspection (overwritten on the next save).
fn load_side<T: DeserializeOwned + Default>(name: &str) -> T {
    let Ok(path) = data_dir().map(|d| d.join(name)) else {
        return T::default();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return T::default();
    };
    serde_json::from_str(&contents).unwrap_or_else(|e| {
        crate::logger::log(
            &format!("{name} load"),
            &format!("{} is corrupt ({e}) — ignoring it", path.display()),
        );
        T::default()
    })
}

/// Write via a temp file + rename so a crash mid-write can't leave a truncated file.
fn save_side<T: Serialize>(name: &str, value: &T) -> Result<()> {
    let contents =
        serde_json::to_string(value).with_context(|| format!("failed to serialize {name}"))?;
    save_side_raw(name, &contents)
}

fn save_side_raw(name: &str, contents: &str) -> Result<()> {
    let path = data_dir()?.join(name);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

pub fn load_session() -> StoredSession {
    load_side("drafts.json")
}

/// Write drafts.json from already-serialized JSON (the caller skips unchanged writes).
pub fn save_session_json(json: &str) -> Result<()> {
    save_side_raw("drafts.json", json)
}

pub fn load_history() -> Vec<HistoryEntry> {
    load_side("history.json")
}

pub fn save_history(history: &[HistoryEntry]) -> Result<()> {
    save_side("history.json", &history)
}

const RESPONSES_FILE: &str = "responses.jsonl";

/// The raw lines of responses.jsonl, oldest first (empty if missing).
pub fn read_response_lines() -> Vec<String> {
    data_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join(RESPONSES_FILE)).ok())
        .map(|c| c.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Request id and summary of one responses.jsonl line, without keeping its body.
/// None for blank or unreadable lines.
pub fn summarize_response_line(line: &str) -> Option<(Uuid, ResponseMeta)> {
    #[derive(Deserialize)]
    struct Summary {
        request_id: Uuid,
        timestamp: chrono::DateTime<chrono::Local>,
        #[serde(default)]
        response: Option<ResponseSummary>,
        #[serde(default)]
        truncated: bool,
    }
    // Unknown fields (headers, body, error) are skipped, not allocated.
    #[derive(Deserialize)]
    struct ResponseSummary {
        status: u16,
        duration_ms: u128,
    }
    let s: Summary = serde_json::from_str(line).ok()?;
    Some((
        s.request_id,
        ResponseMeta {
            timestamp: s.timestamp,
            status: s.response.as_ref().map(|r| r.status),
            duration_ms: s.response.as_ref().map(|r| r.duration_ms),
            truncated: s.truncated,
            offset: 0, // set by the caller, who knows the line's position
        },
    ))
}

/// Append one record and return the byte offset its line starts at.
pub fn append_response(record: &ResponseRecord) -> Result<u64> {
    use std::io::Write;
    let path = data_dir()?.join(RESPONSES_FILE);
    let mut line = serde_json::to_string(record).context("failed to serialize response")?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let offset = file.metadata()?.len();
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to append to {}", path.display()))?;
    Ok(offset)
}

/// Read the full record whose line starts at `offset`.
pub fn read_response_at(offset: u64) -> Result<ResponseRecord> {
    use std::io::{BufRead, Seek, SeekFrom};
    let path = data_dir()?.join(RESPONSES_FILE);
    let mut file =
        std::fs::File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    file.seek(SeekFrom::Start(offset))?;
    let mut line = String::new();
    std::io::BufReader::new(file).read_line(&mut line)?;
    serde_json::from_str(&line).context("unreadable response record")
}

/// Replace the file with `lines` (used to drop old records at startup).
pub fn rewrite_response_lines(lines: &[&str]) -> Result<()> {
    let path = data_dir()?.join(RESPONSES_FILE);
    let tmp = path.with_extension("jsonl.tmp");
    let mut out = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    std::fs::write(&tmp, out).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}
