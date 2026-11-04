use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

fn log_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("postui");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("postui.log"))
}

pub fn log(context: &str, detail: &str) {
    let Some(path) = log_path() else { return };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let _ = writeln!(file, "[{ts}] {context}: {detail}");
}

pub fn path_hint() -> String {
    log_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unavailable>".to_string())
}
