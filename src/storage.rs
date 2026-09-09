use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::model::Workspace;

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
