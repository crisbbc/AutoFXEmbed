//! Persistent settings: selected X/Twitter target, custom domains, per-site
//! toggles, extra site rules, optional X translation language, tracker
//! stripping, and runtime pause state.
//!
//! Small files live under `<config dir>/autofxembed/`:
//! - `x_target`: built-in id or `custom:<domain>`.
//! - `custom_domains.txt`: user-added X/Twitter embed hosts, one per line.
//! - `extra_rules.txt`: generic `source -> target` site rules, one per line.
//! - `strip_trackers`: `0` when tracking parameters should be kept.
//! - `sites.json`: per-site toggles (`x`, `bsky`, `tiktok`) and `x_translate`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

pub use autofxembed_core::{
    is_builtin_domain, normalize_domain, normalize_lang, normalize_site_rule, XTarget,
};

use crate::storage::{config_file, write_atomic};

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

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SiteSettings {
    #[serde(default = "yes")]
    x: bool,
    #[serde(default = "yes")]
    bsky: bool,
    #[serde(default = "yes")]
    tiktok: bool,
    #[serde(default)]
    x_translate: Option<String>,
}

impl SiteSettings {
    const fn new() -> Self {
        Self {
            x: true,
            bsky: true,
            tiktok: true,
            x_translate: None,
        }
    }
}

static SELECTION: RwLock<Selection> = RwLock::new(Selection::Builtin(XTarget::FixUp));
static CUSTOM: RwLock<Vec<String>> = RwLock::new(Vec::new());
static EXTRA_RULES: RwLock<Vec<(String, String)>> = RwLock::new(Vec::new());
static SITE_SETTINGS: RwLock<SiteSettings> = RwLock::new(SiteSettings::new());
static STRIP_TRACKERS: AtomicBool = AtomicBool::new(true);
static PAUSED: AtomicBool = AtomicBool::new(false);

type ChangeHook = Box<dyn Fn() + Send + Sync>;
static ON_CHANGE: OnceLock<ChangeHook> = OnceLock::new();

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

/// Path of the user's custom domain list (one domain per line).
pub fn custom_domains_path() -> Option<PathBuf> {
    config_file("custom_domains.txt")
}

/// Path of the user's generic `source -> target` site rewrite rules.
pub fn extra_rules_path() -> Option<PathBuf> {
    config_file("extra_rules.txt")
}

/// Register a callback run whenever settings change (used to push live updates
/// to the open tray panel and refresh tray state). Only the first call wins.
pub fn set_on_change(hook: impl Fn() + Send + Sync + 'static) {
    let _ = ON_CHANGE.set(Box::new(hook));
}

fn notify_changed() {
    if let Some(hook) = ON_CHANGE.get() {
        hook();
    }
}

/// Runtime pause flag (not persisted across restarts).
pub fn is_paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

pub fn toggle_paused() -> bool {
    let paused = !PAUSED.fetch_xor(true, Ordering::Relaxed);
    eprintln!("AutoFxEmbed: {}", if paused { "paused" } else { "resumed" });
    notify_changed();
    paused
}

/// Currently selected target.
pub fn selection() -> Selection {
    read(&SELECTION).clone()
}

/// Run `f` with the X/Twitter rewrite rules of the current selection.
pub fn with_x_rules<R>(f: impl FnOnce(&[(&str, &str)]) -> R) -> R {
    match &*read(&SELECTION) {
        Selection::Builtin(target) => f(target.rules()),
        Selection::Custom(domain) => f(&autofxembed_core::custom_rules(domain)),
    }
}

/// Whether tracking parameters are removed from copied links (on by default).
pub fn strip_trackers() -> bool {
    STRIP_TRACKERS.load(Ordering::Relaxed)
}

pub fn toggle_strip_trackers() {
    let on = !STRIP_TRACKERS.fetch_xor(true, Ordering::Relaxed);
    if let Some(path) = config_file("strip_trackers") {
        if let Err(error) = write_atomic(&path, if on { "1" } else { "0" }) {
            eprintln!("AutoFxEmbed: unable to save tracker setting: {error}");
        }
    }
    notify_changed();
}

pub fn rewrite_x() -> bool {
    read(&SITE_SETTINGS).x
}

pub fn rewrite_bsky() -> bool {
    read(&SITE_SETTINGS).bsky
}

pub fn rewrite_tiktok() -> bool {
    read(&SITE_SETTINGS).tiktok
}

pub fn x_translate_lang() -> Option<String> {
    read(&SITE_SETTINGS).x_translate.clone()
}

pub fn toggle_rewrite_x() {
    let snapshot = {
        let mut s = write(&SITE_SETTINGS);
        s.x = !s.x;
        s.clone()
    };
    save_site_settings(&snapshot);
    notify_changed();
}

pub fn toggle_rewrite_bsky() {
    let snapshot = {
        let mut s = write(&SITE_SETTINGS);
        s.bsky = !s.bsky;
        s.clone()
    };
    save_site_settings(&snapshot);
    notify_changed();
}

pub fn toggle_rewrite_tiktok() {
    let snapshot = {
        let mut s = write(&SITE_SETTINGS);
        s.tiktok = !s.tiktok;
        s.clone()
    };
    save_site_settings(&snapshot);
    notify_changed();
}

pub fn set_x_translate_lang(input: &str) -> Result<Option<String>, String> {
    let lang = normalize_lang(input).map_err(str::to_string)?;
    let snapshot = {
        let mut s = write(&SITE_SETTINGS);
        s.x_translate = lang.clone();
        s.clone()
    };
    save_site_settings(&snapshot);
    notify_changed();
    Ok(lang)
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
    notify_changed();
}

/// The user's custom X/Twitter domains, in the order they were added.
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
    } else {
        notify_changed();
    }
}

/// Generic user-added `source -> target` site rewrite rules.
pub fn extra_rules() -> Vec<(String, String)> {
    read(&EXTRA_RULES).clone()
}

/// Run `f` with a read lock on the extra site rules.
pub fn with_extra_rules<R>(f: impl FnOnce(&[(String, String)]) -> R) -> R {
    let rules = read(&EXTRA_RULES);
    f(&rules)
}

/// Validate and add (or replace target for) a generic `source -> target` rule.
pub fn add_extra_rule(source: &str, target: &str) -> Result<(String, String), String> {
    let (source, target) = normalize_site_rule(source, target).map_err(str::to_string)?;
    let snapshot = {
        let mut rules = write(&EXTRA_RULES);
        if let Some(existing) = rules.iter_mut().find(|(s, _)| s == &source) {
            existing.1 = target.clone();
        } else {
            rules.push((source.clone(), target.clone()));
        }
        rules.clone()
    };
    if let Err(error) = save_extra_rules(&snapshot) {
        eprintln!("AutoFxEmbed: unable to save extra rules: {error}");
    }
    notify_changed();
    Ok((source, target))
}

/// Forget a generic site rule by its source domain.
pub fn remove_extra_rule(source: &str) {
    let snapshot = {
        let mut rules = write(&EXTRA_RULES);
        rules.retain(|(s, _)| s != source);
        rules.clone()
    };
    if let Err(error) = save_extra_rules(&snapshot) {
        eprintln!("AutoFxEmbed: unable to save extra rules: {error}");
    }
    notify_changed();
}

/// Re-read `custom_domains.txt` and `extra_rules.txt` (so hand edits show up
/// when the panel opens). A selected custom domain that is no longer listed
/// falls back to FixUpX.
pub fn reload_custom() {
    if let Some(path) = custom_domains_path() {
        match std::fs::read_to_string(&path) {
            Ok(text) => *write(&CUSTOM) = parse_custom(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => *write(&CUSTOM) = Vec::new(),
            Err(error) => eprintln!("AutoFxEmbed: unable to read {path:?}: {error}"),
        }
    }
    if matches!(selection(), Selection::Custom(d) if !read(&CUSTOM).contains(&d)) {
        select_builtin(XTarget::FixUp);
    }
    if let Some(path) = extra_rules_path() {
        match std::fs::read_to_string(&path) {
            Ok(text) => *write(&EXTRA_RULES) = parse_extra_rules(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => *write(&EXTRA_RULES) = Vec::new(),
            Err(error) => eprintln!("AutoFxEmbed: unable to read {path:?}: {error}"),
        }
    }
}

/// Load persisted state (call once at startup). Missing/invalid files → defaults.
pub fn load() {
    reload_custom();
    if let Some(path) = config_file("strip_trackers") {
        if let Ok(text) = std::fs::read_to_string(&path) {
            STRIP_TRACKERS.store(text.trim() != "0", Ordering::Relaxed);
        }
    }
    if let Some(path) = config_file("sites.json") {
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<SiteSettings>(&text) {
                Ok(mut loaded) => {
                    loaded.x_translate = loaded
                        .x_translate
                        .as_deref()
                        .and_then(|l| normalize_lang(l).ok().flatten());
                    *write(&SITE_SETTINGS) = loaded;
                }
                Err(error) => eprintln!("AutoFxEmbed: ignoring unreadable sites.json: {error}"),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("AutoFxEmbed: unable to read {path:?}: {error}"),
        }
    }
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

fn save_site_settings(settings: &SiteSettings) {
    let Some(path) = config_file("sites.json") else {
        return;
    };
    let result = serde_json::to_string(settings)
        .map_err(std::io::Error::other)
        .and_then(|json| write_atomic(&path, &json));
    if let Err(error) = result {
        eprintln!("AutoFxEmbed: unable to save site settings: {error}");
    }
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

fn save_extra_rules(rules: &[(String, String)]) -> std::io::Result<()> {
    let Some(path) = extra_rules_path() else {
        return Ok(());
    };
    let mut text = rules
        .iter()
        .map(|(source, target)| format!("{source} -> {target}"))
        .collect::<Vec<_>>()
        .join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    write_atomic(&path, &text)
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

/// Valid, de-duplicated `source -> target` rules from `extra_rules.txt`.
fn parse_extra_rules(text: &str) -> Vec<(String, String)> {
    let mut rules: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((src, dst)) = trimmed
            .split_once("->")
            .or_else(|| trimmed.split_once('='))
            .or_else(|| trimmed.split_once(char::is_whitespace))
        else {
            continue;
        };
        if let Ok((source, target)) = normalize_site_rule(src, dst) {
            if !rules.iter().any(|(s, _)| s == &source) {
                rules.push((source, target));
            }
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn extra_rules_file_parses_valid_pairs() {
        let text = "\
# comment
reddit.com -> rxddit.com
https://Instagram.com/ = kkinstagram.com
reddit.com -> duplicate.com
x.com -> nope.com
badline
";
        assert_eq!(
            parse_extra_rules(text),
            vec![
                ("reddit.com".to_string(), "rxddit.com".to_string()),
                ("instagram.com".to_string(), "kkinstagram.com".to_string()),
            ]
        );
    }
}
