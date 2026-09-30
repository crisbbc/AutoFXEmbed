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
    /// Every target, in discriminant order (`ALL[t as usize] == t`).
    pub const ALL: [XTarget; 3] = [XTarget::FixUp, XTarget::BoyPussyX, XTarget::MpregX];

    /// Stable identifier persisted in the config file.
    pub fn id(self) -> &'static str {
        match self {
            XTarget::FixUp => "fixup",
            XTarget::BoyPussyX => "boypussyx",
            XTarget::MpregX => "mpregx",
        }
    }

    /// Human-readable tray menu label.
    pub fn label(self) -> &'static str {
        match self {
            XTarget::FixUp => "FixUpX (fxtwitter / fixupx)",
            XTarget::BoyPussyX => "BoyPussyX (boypussyx.com)",
            XTarget::MpregX => "MpregX (mpregx.com)",
        }
    }

    /// X/Twitter host rewrite rules for this target.
    pub fn rules(self) -> &'static [(&'static str, &'static str)] {
        match self {
            XTarget::FixUp => crate::transform::FIXUP_RULES,
            XTarget::BoyPussyX => crate::transform::BOY_RULES,
            XTarget::MpregX => crate::transform::MPREG_RULES,
        }
    }

    fn from_id(s: &str) -> XTarget {
        Self::ALL
            .into_iter()
            .find(|target| target.id() == s)
            .unwrap_or(XTarget::FixUp)
    }

    fn from_u8(value: u8) -> XTarget {
        Self::ALL
            .get(value as usize)
            .copied()
            .unwrap_or(XTarget::FixUp)
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
    eprintln!("AutoFxEmbed: X target -> {}", target.id());
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
    std::fs::write(path, target.id())
}

/// Load persisted choice (call once at startup). Missing/invalid file → FixUpX.
pub fn load() {
    let Some(path) = config_path() else {
        eprintln!("AutoFxEmbed: no config_dir");
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let target = XTarget::from_id(s.trim());
            eprintln!("AutoFxEmbed: load {:?} -> {}", path, target.id());
            X_TARGET.store(target as u8, Ordering::Relaxed);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("AutoFxEmbed: load {:?} err {e}", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_discriminant_order() {
        for target in XTarget::ALL {
            assert_eq!(XTarget::from_u8(target as u8), target);
            assert_eq!(XTarget::from_id(target.id()), target);
        }
    }
}
