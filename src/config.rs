//! Selected FxEmbed target for X/Twitter domains.
//! Persists a single value ("fixup" | "boypussyx") under the OS config dir.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static USE_BOY: AtomicBool = AtomicBool::new(false);


fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("autofxembed").join("x_target"))
}

/// `true` if BoyPussyX (`boypussyx.com`) is selected, `false` for FixUpX.
pub fn is_boypussyx() -> bool {
    USE_BOY.load(Ordering::Relaxed)
}

pub fn set_boypussyx(value: bool) {
    USE_BOY.store(value, Ordering::Relaxed);
    eprintln!("AutoFxEmbed: X target -> {}", if value { "boypussyx" } else { "fixup" });
    let _ = save(value);
}

fn save(value: bool) -> std::io::Result<()> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = if value { "boypussyx" } else { "fixup" };
    std::fs::write(path, content)
}

/// Load persisted choice (call once at startup). Missing/invalid file → FixUpX.
pub fn load() {
    let Some(path) = config_path() else {
        eprintln!("AutoFxEmbed: no config_dir");
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let is_boy = s.trim() == "boypussyx";
            eprintln!("AutoFxEmbed: load {:?} -> is_boy={}", path, is_boy);
            USE_BOY.store(is_boy, Ordering::Relaxed);
        }
        Err(e) => eprintln!("AutoFxEmbed: load {:?} err {e}", path),
    }
}

#[cfg(test)]
pub fn set_for_tests(value: bool) {
    USE_BOY.store(value, Ordering::Relaxed);
}
