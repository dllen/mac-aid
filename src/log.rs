use std::fs::{rename, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 128 * 1024 * 1024; // 128 MB

fn get_log_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".mac-aid");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("error.log"))
}

/// Append an error message to the error log, rotating when exceeding MAX_LOG_BYTES.
pub fn log_error(msg: &str) {
    if let Some(path) = get_log_path() {
        // If file exists and is too large, rotate
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() >= MAX_LOG_BYTES {
                // rotate to error.log.1 (overwrite if exists)
                let rotated = path.with_extension("log.1");
                // best-effort rename; if fails, try to remove then rename
                if let Err(_) = rename(&path, &rotated) {
                    let _ = std::fs::remove_file(&rotated);
                    let _ = rename(&path, &rotated);
                }
            }
        }

        // Open for append
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(mut f) => {
                // Add a simple timestamp (seconds since epoch)
                let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = writeln!(f, "[{}] {}", ts, msg);
            }
            Err(_) => {
                // Last resort: write to stderr
                let _ = std::io::stderr().write_all(msg.as_bytes());
                let _ = std::io::stderr().write_all(b"\n");
            }
        }
    } else {
        // Could not determine log path, fallback to stderr
        let _ = std::io::stderr().write_all(msg.as_bytes());
        let _ = std::io::stderr().write_all(b"\n");
    }
}
