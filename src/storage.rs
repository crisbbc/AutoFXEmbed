//! Shared config-directory helpers and atomic file writes.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Directory where AutoFxEmbed persists its config, history, and update state.
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("autofxembed"))
}

/// Path to `name` inside AutoFxEmbed's config directory.
pub fn config_file(name: &str) -> Option<PathBuf> {
    config_dir().map(|p| p.join(name))
}

/// Create a uniquely named sibling temp file with `O_EXCL` (`create_new`), write
/// `contents`, and atomically rename it onto `path`.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("autofxembed");
    let pid = std::process::id();
    let mut tmp = None;
    let mut file = None;
    for _ in 0..16 {
        let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = path.with_file_name(format!(".{file_name}.{pid}.{seq}.tmp"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => {
                tmp = Some(candidate);
                file = Some(f);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    let (Some(tmp), Some(mut file)) = (tmp, file) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "unable to allocate unique temp file",
        ));
    };
    let write_res = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    if let Err(err) = write_res {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}
