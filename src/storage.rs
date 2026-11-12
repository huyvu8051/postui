use anyhow::{Context, Result};
use std::path::PathBuf;

use serde::{de::DeserializeOwned, Serialize};

use crate::model::{HistoryEntry, ResponseRecord, Workspace};
use crate::session::StoredSession;

fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .context("could not determine data directory")?
        .join("postui");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
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
    let contents = serde_json::to_string_pretty(workspace).context("failed to serialize workspace")?;
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
        crate::logger::log(&format!("{name} load"), &format!("{} is corrupt ({e}) — ignoring it", path.display()));
        T::default()
    })
}

/// Write via a temp file + rename so a crash mid-write can't leave a truncated file.
fn save_side<T: Serialize>(name: &str, value: &T) -> Result<()> {
    let path = data_dir()?.join(name);
    let tmp = path.with_extension("json.tmp");
    let contents = serde_json::to_string(value).with_context(|| format!("failed to serialize {name}"))?;
    std::fs::write(&tmp, contents).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

pub fn load_session() -> StoredSession {
    load_side("drafts.json")
}

pub fn save_session(session: &StoredSession) -> Result<()> {
    save_side("drafts.json", session)
}

pub fn load_history() -> Vec<HistoryEntry> {
    load_side("history.json")
}

pub fn save_history(history: &[HistoryEntry]) -> Result<()> {
    save_side("history.json", &history)
}

const RESPONSES_FILE: &str = "responses.jsonl";

/// All stored responses, oldest first. Unreadable lines are skipped (and logged).
pub fn load_responses() -> Vec<ResponseRecord> {
    let Ok(path) = data_dir().map(|d| d.join(RESPONSES_FILE)) else {
        return Vec::new();
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut bad = 0;
    let records = contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).map_err(|_| bad += 1).ok())
        .collect();
    if bad > 0 {
        crate::logger::log("responses load", &format!("skipped {bad} unreadable line(s) in {}", path.display()));
    }
    records
}

/// Append one record — cheap even when the file is large.
pub fn append_response(record: &ResponseRecord) -> Result<()> {
    use std::io::Write;
    let path = data_dir()?.join(RESPONSES_FILE);
    let mut line = serde_json::to_string(record).context("failed to serialize response")?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()))
        .with_context(|| format!("failed to append to {}", path.display()))
}

/// Rewrite the whole file (used to drop old records at startup).
pub fn rewrite_responses(records: &[ResponseRecord]) -> Result<()> {
    let path = data_dir()?.join(RESPONSES_FILE);
    let tmp = path.with_extension("jsonl.tmp");
    let mut out = String::new();
    for r in records {
        out.push_str(&serde_json::to_string(r).context("failed to serialize response")?);
        out.push('\n');
    }
    std::fs::write(&tmp, out).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}
