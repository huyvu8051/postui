//! Helpers shared by tests.
use uuid::Uuid;

/// Everything that touches disk goes to a throwaway HOME, never the real workspace.
/// Tests holding the returned guard run one at a time and start with empty storage.
pub fn isolated_storage() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static HOME: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = HOME.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("postui-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: set once, before any test reads storage paths (all go through this lock).
        unsafe { std::env::set_var("HOME", &home) };
        home
    });
    let data = dirs::data_dir().unwrap();
    assert!(data.starts_with(home), "storage must point at the temp HOME");
    let _ = std::fs::remove_dir_all(data.join("postui"));
    guard
}
