use std::fs::{rename, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 128 * 1024 * 1024; // 128 MB
const MAX_LOG_BACKUPS: usize = 5; // number of rotated archives to keep

fn get_log_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".mac-aid");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("error.log"))
}

fn get_info_log_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let dir = home.join(".mac-aid");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("info.log"))
}

fn rotate_backups(base: &PathBuf, max_backups: usize) {
    // base is like /.../error.log or info.log
    // We want to move: base.(max_backups-1) -> base.max_backups, ... base.1 -> base.2, base -> base.1
    // Use best-effort: ignore errors, but attempt to remove existing target if rename fails
    for i in (1..=max_backups).rev() {
        let src = if i == 1 {
            base.clone()
        } else {
            base.with_extension(format!("log.{}", i - 1))
        };

        let dst = base.with_extension(format!("log.{}", i));

        if src.exists() {
            // If dst exists, try to remove it first to allow rename
            if dst.exists() {
                let _ = std::fs::remove_file(&dst);
            }
            // Try rename; on failure, attempt copy then remove
            if let Err(_) = rename(&src, &dst) {
                // fallback: try copy and remove
                if let (Ok(mut r), Ok(mut w)) = (
                    std::fs::File::open(&src),
                    OpenOptions::new().create(true).write(true).open(&dst),
                ) {
                    use std::io::copy;
                    let _ = copy(&mut r, &mut w);
                    let _ = std::fs::remove_file(&src);
                }
            }
        }
    }
}

/// Append an error message to the error log, rotating when exceeding MAX_LOG_BYTES.
pub fn log_error(msg: &str) {
    if let Some(path) = get_log_path() {
        // If file exists and is too large, rotate keeping multiple backups
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() >= MAX_LOG_BYTES {
                rotate_backups(&path, MAX_LOG_BACKUPS);
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

/// Append an info message to the info log, rotating when exceeding MAX_LOG_BYTES.
pub fn log_info(msg: &str) {
    if let Some(path) = get_info_log_path() {
        // If file exists and is too large, rotate keeping multiple backups
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() >= MAX_LOG_BYTES {
                rotate_backups(&path, MAX_LOG_BACKUPS);
            }
        }

        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(mut f) => {
                let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = writeln!(f, "[{}] {}", ts, msg);
            }
            Err(_) => {
                let _ = std::io::stderr().write_all(msg.as_bytes());
                let _ = std::io::stderr().write_all(b"\n");
            }
        }
    } else {
        let _ = std::io::stderr().write_all(msg.as_bytes());
        let _ = std::io::stderr().write_all(b"\n");
    }
}
