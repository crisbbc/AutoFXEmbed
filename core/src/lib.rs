//! Logic shared by the desktop app and the Android app: rewriting links to
//! embed-friendly hosts and stripping trackers, validating custom domains,
//! reading embed data for the Recent list and comparing release versions.
//! Pure string handling, no I/O.

mod domains;
pub mod embed;
mod trackers;
mod transform;
mod version;

pub use domains::{custom_rules, is_builtin_domain, normalize_domain};
pub use version::{is_newer, parse_version};

pub use transform::{
    transform_clipboard, transform_text, transform_text_links, Options, Rewrite, BOY_RULES,
    FAGGOT_RULES, FIXUP_RULES, MPREG_RULES, YAOI_RULES,
};

/// Which embed host X/Twitter links are rewritten to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XTarget {
    /// FixUpX (`fxtwitter.com` / `fixupx.com`).
    FixUp = 0,
    /// BoyPussyX (`boypussyx.com`).
    BoyPussyX = 1,
    /// MpregX (`mpregx.com`).
    MpregX = 2,
    /// YaoiSex (`yaoisex.com`).
    YaoiSex = 3,
    /// FaggotX (`faggotx.com`).
    FaggotX = 4,
}

impl XTarget {
    /// Every target, in discriminant order (`ALL[t as usize] == t`).
    pub const ALL: [XTarget; 5] = [
        XTarget::FixUp,
        XTarget::BoyPussyX,
        XTarget::MpregX,
        XTarget::YaoiSex,
        XTarget::FaggotX,
    ];

    /// Stable identifier persisted in the config file.
    pub fn id(self) -> &'static str {
        match self {
            XTarget::FixUp => "fixup",
            XTarget::BoyPussyX => "boypussyx",
            XTarget::MpregX => "mpregx",
            XTarget::YaoiSex => "yaoisex",
            XTarget::FaggotX => "faggotx",
        }
    }

    /// Human-readable tray menu label.
    pub fn label(self) -> &'static str {
        match self {
            XTarget::FixUp => "FixUpX (fxtwitter / fixupx)",
            XTarget::BoyPussyX => "BoyPussyX (boypussyx.com)",
            XTarget::MpregX => "MpregX (mpregx.com)",
            XTarget::YaoiSex => "YaoiSex (yaoisex.com)",
            XTarget::FaggotX => "FaggotX (faggotx.com)",
        }
    }

    /// X/Twitter host rewrite rules for this target.
    pub fn rules(self) -> &'static [(&'static str, &'static str)] {
        match self {
            XTarget::FixUp => crate::FIXUP_RULES,
            XTarget::BoyPussyX => crate::BOY_RULES,
            XTarget::MpregX => crate::MPREG_RULES,
            XTarget::YaoiSex => crate::YAOI_RULES,
            XTarget::FaggotX => crate::FAGGOT_RULES,
        }
    }

    /// The target persisted as `s` by [`XTarget::id`].
    pub fn from_id(s: &str) -> Option<XTarget> {
        Self::ALL.into_iter().find(|target| target.id() == s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_discriminant_order() {
        for (index, target) in XTarget::ALL.into_iter().enumerate() {
            assert_eq!(target as usize, index);
            assert_eq!(XTarget::from_id(target.id()), Some(target));
        }
    }
}
