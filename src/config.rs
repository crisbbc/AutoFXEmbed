//! Selected embed host for X/Twitter links, plus user-added custom domains.
//!
//! Two small files live under the OS config dir (`autofxembed/`):
//! - `x_target`: the selection, a built-in id ("fixup" | "boypussyx" | "mpregx" |
//!   "yaoisex" | "faggotx") or `custom:<domain>`.
//! - `custom_domains.txt`: the user's own embed hosts, one domain per line.

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

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
            XTarget::FixUp => crate::transform::FIXUP_RULES,
            XTarget::BoyPussyX => crate::transform::BOY_RULES,
            XTarget::MpregX => crate::transform::MPREG_RULES,
            XTarget::YaoiSex => crate::transform::YAOI_RULES,
            XTarget::FaggotX => crate::transform::FAGGOT_RULES,
        }
    }

    fn from_id(s: &str) -> Option<XTarget> {
        Self::ALL.into_iter().find(|target| target.id() == s)
    }
}

/// What X/Twitter links are currently rewritten to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Builtin(XTarget),
    /// A user-added embed host (already normalized).
    Custom(String),
}

const CUSTOM_PREFIX: &str = "custom:";

impl Selection {
    fn id(&self) -> String {
        match self {
            Selection::Builtin(target) => target.id().to_string(),
            Selection::Custom(domain) => format!("{CUSTOM_PREFIX}{domain}"),
        }
    }

    /// Unknown or malformed ids fall back to FixUpX.
    fn from_id(s: &str) -> Selection {
        if let Some(domain) = s.strip_prefix(CUSTOM_PREFIX) {
            if let Ok(domain) = normalize_domain(domain) {
                return Selection::Custom(domain);
            }
        }
        Selection::Builtin(XTarget::from_id(s).unwrap_or(XTarget::FixUp))
    }
}

static SELECTION: RwLock<Selection> = RwLock::new(Selection::Builtin(XTarget::FixUp));
static CUSTOM: RwLock<Vec<String>> = RwLock::new(Vec::new());
type ChangeHook = Box<dyn Fn() + Send + Sync>;
static ON_CHANGE: OnceLock<ChangeHook> = OnceLock::new();

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

fn config_file(name: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("autofxembed").join(name))
}

/// Path of the user's custom domain list (one domain per line).
pub fn custom_domains_path() -> Option<PathBuf> {
    config_file("custom_domains.txt")
}

/// Register a callback run after the custom domain list changes (used by the
/// Linux tray to refresh its menu). Only the first call wins.
pub fn set_on_change(hook: impl Fn() + Send + Sync + 'static) {
    let _ = ON_CHANGE.set(Box::new(hook));
}

fn notify_changed() {
    if let Some(hook) = ON_CHANGE.get() {
        hook();
    }
}

/// Currently selected target.
pub fn selection() -> Selection {
    read(&SELECTION).clone()
}

/// Run `f` with the X/Twitter rewrite rules of the current selection.
pub fn with_x_rules<R>(f: impl FnOnce(&[(&str, &str)]) -> R) -> R {
    match &*read(&SELECTION) {
        Selection::Builtin(target) => f(target.rules()),
        Selection::Custom(domain) => f(&[("twitter.com", domain), ("x.com", domain)]),
    }
}

pub fn select_builtin(target: XTarget) {
    set_selection(Selection::Builtin(target));
}

pub fn select_custom(domain: &str) {
    set_selection(Selection::Custom(domain.to_string()));
}

fn set_selection(selection: Selection) {
    eprintln!("AutoFxEmbed: X target -> {}", selection.id());
    if let Err(error) = save_selection(&selection) {
        eprintln!("AutoFxEmbed: unable to save X target: {error}");
    }
    *write(&SELECTION) = selection;
}

/// The user's custom domains, in the order they were added.
pub fn custom_domains() -> Vec<String> {
    read(&CUSTOM).clone()
}

/// Validate and add a custom embed domain, then select it. Returns the
/// normalized domain, or a message explaining why the input was rejected.
pub fn add_custom(input: &str) -> Result<String, String> {
    let domain = normalize_domain(input).map_err(str::to_string)?;
    if is_builtin_domain(&domain) {
        return Err(format!("{domain} is already built in"));
    }
    let snapshot = {
        let mut customs = write(&CUSTOM);
        if !customs.contains(&domain) {
            customs.push(domain.clone());
        }
        customs.clone()
    };
    if let Err(error) = save_custom(&snapshot) {
        eprintln!("AutoFxEmbed: unable to save custom domains: {error}");
    }
    select_custom(&domain);
    notify_changed();
    Ok(domain)
}

/// Forget a custom domain; if it was selected, fall back to FixUpX.
pub fn remove_custom(domain: &str) {
    let snapshot = {
        let mut customs = write(&CUSTOM);
        customs.retain(|d| d != domain);
        customs.clone()
    };
    if let Err(error) = save_custom(&snapshot) {
        eprintln!("AutoFxEmbed: unable to save custom domains: {error}");
    }
    if matches!(selection(), Selection::Custom(d) if d == domain) {
        select_builtin(XTarget::FixUp);
    }
    notify_changed();
}

/// Re-read `custom_domains.txt` (so hand edits show up when the menu opens).
/// A selected custom domain that is no longer listed falls back to FixUpX.
pub fn reload_custom() {
    let Some(path) = custom_domains_path() else {
        return;
    };
    let customs = match std::fs::read_to_string(&path) {
        Ok(text) => parse_custom(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            eprintln!("AutoFxEmbed: unable to read {path:?}: {error}");
            return;
        }
    };
    *write(&CUSTOM) = customs;
    if matches!(selection(), Selection::Custom(d) if !read(&CUSTOM).contains(&d)) {
        select_builtin(XTarget::FixUp);
    }
}

/// Load persisted state (call once at startup). Missing/invalid files → FixUpX
/// and no custom domains.
pub fn load() {
    reload_custom();
    let Some(path) = config_file("x_target") else {
        eprintln!("AutoFxEmbed: no config_dir");
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let mut selection = Selection::from_id(s.trim());
            if matches!(&selection, Selection::Custom(d) if !read(&CUSTOM).contains(d)) {
                selection = Selection::Builtin(XTarget::FixUp);
            }
            eprintln!("AutoFxEmbed: load {:?} -> {}", path, selection.id());
            *write(&SELECTION) = selection;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("AutoFxEmbed: load {:?} err {e}", path),
    }
}

fn save_selection(selection: &Selection) -> std::io::Result<()> {
    let Some(path) = config_file("x_target") else {
        return Ok(());
    };
    write_atomic(&path, &selection.id())
}

fn save_custom(domains: &[String]) -> std::io::Result<()> {
    let Some(path) = custom_domains_path() else {
        return Ok(());
    };
    let mut text = domains.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    write_atomic(&path, &text)
}

/// Write-then-rename so a crash never leaves a half-written file.
fn write_atomic(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Valid, de-duplicated domains from the text of `custom_domains.txt`. Blank
/// lines, `#` comments and invalid entries are skipped.
fn parse_custom(text: &str) -> Vec<String> {
    let mut domains = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        match normalize_domain(line) {
            Ok(domain) if !is_builtin_domain(&domain) && !domains.contains(&domain) => {
                domains.push(domain);
            }
            _ => {}
        }
    }
    domains
}

/// True if `host` is `domain` or a subdomain of it.
fn is_or_under(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|p| p.ends_with('.'))
}

/// True for the embed hosts that already have a built-in target.
pub fn is_builtin_domain(domain: &str) -> bool {
    XTarget::ALL
        .into_iter()
        .flat_map(|target| target.rules())
        .any(|&(_, replacement)| replacement == domain)
}

/// Turn what a user typed (`https://MyFx.com/path`) into a bare lowercase
/// domain (`myfx.com`), or explain why it can't be used as an embed host.
pub fn normalize_domain(input: &str) -> Result<String, &'static str> {
    let mut text = input.trim().to_ascii_lowercase();
    for scheme in ["https://", "http://"] {
        if let Some(rest) = text.strip_prefix(scheme) {
            text = rest.to_string();
            break;
        }
    }
    text.truncate(text.find(['/', '?', '#', ':']).unwrap_or(text.len()));
    let domain = text.trim_end_matches('.');

    if domain.is_empty() {
        return Err("Enter a domain such as example.com");
    }
    if domain.len() > 253 || !domain.contains('.') {
        return Err("That doesn't look like a domain name");
    }
    let valid_label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    if !domain.split('.').all(valid_label) {
        return Err("That doesn't look like a domain name");
    }
    // The embed host must not itself be rewritten, or links would loop.
    if is_or_under(domain, "x.com") || is_or_under(domain, "twitter.com") {
        return Err("That domain would be rewritten again");
    }
    Ok(domain.to_string())
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

    #[test]
    fn normalizes_what_users_type() {
        for (input, expected) in [
            ("MyFx.COM", "myfx.com"),
            ("  https://myfx.com/path?q=1  ", "myfx.com"),
            ("http://sub.myfx.com:8080/", "sub.myfx.com"),
            ("myfx.com.", "myfx.com"),
            ("my-fx2.example.org", "my-fx2.example.org"),
        ] {
            assert_eq!(normalize_domain(input).as_deref(), Ok(expected), "{input}");
        }
    }

    #[test]
    fn rejects_unusable_domains() {
        for input in [
            "",
            "   ",
            "myfx",
            "x.com",
            "https://twitter.com/foo",
            "fx.twitter.com",
            "bad_domain.com",
            "-a.com",
            "a-.com",
            "a..com",
            "user@myfx.com",
            "myfx .com",
            "ünï.com",
        ] {
            assert!(normalize_domain(input).is_err(), "{input:?} should fail");
        }
        // A look-alike that merely ends in the same letters is fine.
        assert_eq!(normalize_domain("myx.com").as_deref(), Ok("myx.com"));
    }

    #[test]
    fn builtin_hosts_are_not_custom() {
        for domain in ["fxtwitter.com", "fixupx.com", "mpregx.com", "faggotx.com"] {
            assert!(is_builtin_domain(domain), "{domain}");
        }
        assert!(!is_builtin_domain("myfx.com"));
    }

    #[test]
    fn selection_ids_round_trip() {
        for target in XTarget::ALL {
            let selection = Selection::Builtin(target);
            assert_eq!(Selection::from_id(&selection.id()), selection);
        }
        let custom = Selection::Custom("myfx.com".into());
        assert_eq!(custom.id(), "custom:myfx.com");
        assert_eq!(Selection::from_id("custom:myfx.com"), custom);
        // Unknown / malformed ids fall back to FixUpX.
        for id in ["", "nope", "custom:", "custom:x.com"] {
            assert_eq!(
                Selection::from_id(id),
                Selection::Builtin(XTarget::FixUp),
                "{id:?}"
            );
        }
    }

    #[test]
    fn custom_file_skips_junk_and_duplicates() {
        let text =
            "# my hosts\nmyfx.com\n\nMYFX.com\nnot a domain\nfixupx.com\nhttps://other.net/\n";
        assert_eq!(parse_custom(text), ["myfx.com", "other.net"]);
    }
}
