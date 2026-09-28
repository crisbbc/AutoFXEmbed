//! Selected FxEmbed target for X/Twitter domains.
//! Persists a single value ("fixup" | "boypussyx" | "mpregx") under the OS config dir.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

/// Which embed host X/Twitter links are rewritten to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XTarget {
    /// FixUpX (`fxtwitter.com` / `fixupx.com`).
    FixUp = 0,
    /// BoyPussyX (`boypussyx.com`).
    BoyPussyX = 1,
    /// MpregX (`mpregx.com`).
    MpregX = 2,
}

impl XTarget {
    fn as_str(self) -> &'static str {
        match self {
            XTarget::FixUp => "fixup",
            XTarget::BoyPussyX => "boypussyx",
            XTarget::MpregX => "mpregx",
        }
    }

    fn from_str(s: &str) -> XTarget {
        match s {
            "boypussyx" => XTarget::BoyPussyX,
            "mpregx" => XTarget::MpregX,
            _ => XTarget::FixUp,
        }
    }

    fn from_u8(value: u8) -> XTarget {
        match value {
            1 => XTarget::BoyPussyX,
            2 => XTarget::MpregX,
            _ => XTarget::FixUp,
        }
    }
}

static X_TARGET: AtomicU8 = AtomicU8::new(XTarget::FixUp as u8);

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("autofxembed").join("x_target"))
}

/// Currently selected X/Twitter target.
pub fn x_target() -> XTarget {
    XTarget::from_u8(X_TARGET.load(Ordering::Relaxed))
}

pub fn set_x_target(target: XTarget) {
    X_TARGET.store(target as u8, Ordering::Relaxed);
    eprintln!("AutoFxEmbed: X target -> {}", target.as_str());
    if let Err(error) = save(target) {
        eprintln!("AutoFxEmbed: unable to save X target: {error}");
    }
}

fn save(target: XTarget) -> std::io::Result<()> {
    let Some(path) = config_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, target.as_str())
}

/// Load persisted choice (call once at startup). Missing/invalid file → FixUpX.
pub fn load() {
    let Some(path) = config_path() else {
        eprintln!("AutoFxEmbed: no config_dir");
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let target = XTarget::from_str(s.trim());
            eprintln!("AutoFxEmbed: load {:?} -> {}", path, target.as_str());
            X_TARGET.store(target as u8, Ordering::Relaxed);
        }
        Err(e) => eprintln!("AutoFxEmbed: load {:?} err {e}", path),
    }
}
