use std::fs::{self, OpenOptions};
use std::io::Write;

use crate::runtime::paths;

fn ensure_log_parent() -> Result<(), String> {
    let path = paths::log_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create log dir: {e}"))?;
    }
    Ok(())
}

pub fn append_line(message: impl AsRef<str>) -> Result<(), String> {
    ensure_log_parent()?;
    let path = paths::log_file_path();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?;

    writeln!(file, "{}", message.as_ref())
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

pub fn clear() -> Result<(), String> {
    ensure_log_parent()?;
    let path = paths::log_file_path();
    fs::write(&path, "").map_err(|e| format!("Failed to clear {}: {e}", path.display()))
}
