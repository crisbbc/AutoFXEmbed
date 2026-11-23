//! Self-updater: checks the latest GitHub release, and on the user's say-so
//! downloads the matching asset, verifies its SHA-256 against the digest GitHub
//! publishes, replaces the running binary and restarts.
//!
//! Checks run on worker threads so the tray / clipboard loops never wait on the
//! network. State (auto-check toggle, last check, declined version) lives in
//! `<config dir>/autofxembed/updates.json`.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use autofxembed_core::is_newer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::storage::{config_dir, config_file, write_atomic};

const LATEST_URL: &str = "https://api.github.com/repos/crisbbc/AutoFXEmbed/releases/latest";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const STARTUP_DELAY: Duration = Duration::from_secs(30);
/// How often the background thread re-evaluates whether a check is due
/// (so a suspended laptop still checks soon after waking).
const TICK: Duration = Duration::from_secs(60 * 60);
const API_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_API_BYTES: u64 = 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
const CARGO_INSTALL_COMMAND: &str =
    "cargo install --git https://github.com/crisbbc/AutoFXEmbed.git --locked";

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    #[serde(default = "yes")]
    auto_check: bool,
    /// Unix time (seconds) of the last successful check.
    #[serde(default)]
    last_check: u64,
    /// Version the user declined from an automatic check.
    #[serde(default)]
    skipped: Option<String>,
}

impl State {
    const fn new() -> Self {
        State {
            auto_check: true,
            last_check: 0,
            skipped: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Auto,
    Manual,
}

static STATE: Mutex<State> = Mutex::new(State::new());
/// Only one check / download at a time.
static CHECKING: AtomicBool = AtomicBool::new(false);
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);
/// Set once an update is installed: the executable to relaunch after exit.
static RESTART: OnceLock<PathBuf> = OnceLock::new();
type ChangeHook = Box<dyn Fn() + Send + Sync>;
static ON_CHANGE: OnceLock<ChangeHook> = OnceLock::new();

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

fn state_path() -> Option<PathBuf> {
    config_file("updates.json")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Register a callback run when update status or settings change.
pub fn set_on_change(hook: impl Fn() + Send + Sync + 'static) {
    let _ = ON_CHANGE.set(Box::new(hook));
}

fn notify_changed() {
    if let Some(hook) = ON_CHANGE.get() {
        hook();
    }
}

/// True while an update check or download is currently in progress.
pub fn is_checking() -> bool {
    CHECKING.load(Ordering::SeqCst)
}

/// Load the persisted state (call once at startup). Missing/corrupt file → defaults.
pub fn load() {
    let Some(path) = state_path() else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(loaded) => *state() = loaded,
            Err(error) => eprintln!("AutoFxEmbed: ignoring unreadable update state: {error}"),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!("AutoFxEmbed: unable to read update state: {error}"),
    }
}

fn save(state: &State) {
    let Some(path) = state_path() else {
        return;
    };
    let result = serde_json::to_string(state)
        .map_err(std::io::Error::other)
        .and_then(|json| write_atomic(&path, &json));
    if let Err(error) = result {
        eprintln!("AutoFxEmbed: unable to save update state: {error}");
    }
}

/// Whether automatic checks are on (the tray checkmark).
pub fn auto_enabled() -> bool {
    state().auto_check
}

pub fn toggle_auto() {
    {
        let mut state = state();
        state.auto_check = !state.auto_check;
        save(&state);
    }
    notify_changed();
}

/// The executable to relaunch once the app has exited after an update.
pub fn restart_target() -> Option<PathBuf> {
    RESTART.get().cloned()
}

/// Start the background checker: first check shortly after launch, then daily.
pub fn start() {
    std::thread::spawn(|| {
        std::thread::sleep(STARTUP_DELAY);
        loop {
            if auto_check_due(&state(), now()) {
                check(Trigger::Auto);
            }
            std::thread::sleep(TICK);
        }
    });
}

/// Manual "Check for updates...". Returns immediately.
pub fn check_now() {
    std::thread::spawn(|| check(Trigger::Manual));
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

fn find_asset<'a>(release: &'a Release, name: &str) -> Option<&'a Asset> {
    release.assets.iter().find(|asset| asset.name == name)
}

/// `sha256:<64 hex chars>` → the raw hash.
fn sha256_from_digest(digest: &str) -> Option<[u8; 32]> {
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (byte, pair) in out.iter_mut().zip(hex.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

fn auto_check_due(state: &State, now: u64) -> bool {
    state.auto_check && now.saturating_sub(state.last_check) >= CHECK_INTERVAL.as_secs()
}

/// True when the binary lives in Cargo's bin dir, where `cargo install` owns it.
fn is_cargo_install(exe: &Path) -> bool {
    let Some(dir) = exe.parent() else {
        return false;
    };
    dir.ends_with(".cargo/bin")
        || std::env::var_os("CARGO_HOME").is_some_and(|home| dir == Path::new(&home).join("bin"))
}

/// The release asset built for this platform.
fn asset_name() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("autofxembed-linux-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("autofxembed-linux-aarch64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("autofxembed-windows-x86_64.exe")
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Some("autofxembed-windows-aarch64.exe")
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Check + install
// ---------------------------------------------------------------------------

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("AutoFxEmbed/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn fetch_latest() -> Result<Release, String> {
    let response = agent(API_TIMEOUT)
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let mut text = String::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_API_BYTES)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn download(asset: &Asset, expected: [u8; 32]) -> Result<Vec<u8>, String> {
    if !asset.browser_download_url.starts_with("https://") {
        return Err("refusing non-HTTPS download URL".into());
    }
    if asset.size == 0 || asset.size > MAX_ASSET_BYTES {
        return Err(format!("unexpected download size ({} bytes)", asset.size));
    }
    let response = agent(DOWNLOAD_TIMEOUT)
        .get(&asset.browser_download_url)
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::with_capacity(asset.size as usize);
    response
        .into_body()
        .into_reader()
        .take(asset.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != asset.size {
        return Err("downloaded size does not match the release".into());
    }
    if Sha256::digest(&bytes).as_slice() != expected {
        return Err("checksum mismatch".into());
    }
    Ok(bytes)
}

/// Create an exclusive temporary file in a private directory (preferring the
/// running binary's own directory, then config dir, then OS temp dir) so
/// shared `/tmp` races are avoided and renames stay on the same filesystem.
fn create_staging_file(exe: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    let mut dirs = Vec::new();
    if let Some(parent) = exe.parent() {
        dirs.push(parent.to_path_buf());
    }
    if let Some(cfg) = config_dir() {
        let _ = std::fs::create_dir_all(&cfg);
        dirs.push(cfg);
    }
    dirs.push(std::env::temp_dir());

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());

    let mut last_err = None;
    for dir in dirs {
        for _ in 0..8 {
            let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let candidate = dir.join(format!(".autofxembed-update-{pid}-{nanos}-{seq}.tmp"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => return Ok((candidate, file)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| std::io::Error::other("unable to create staging file for update")))
}

/// Swap the running executable for `bytes`.
fn install(bytes: &[u8], exe: &Path) -> Result<(), String> {
    let (tmp, mut file) = create_staging_file(exe).map_err(|e| e.to_string())?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        self_replace::self_replace(&tmp)
    })();
    let _ = std::fs::remove_file(&tmp);
    result.map_err(|e| e.to_string())
}

struct CheckingGuard;
impl Drop for CheckingGuard {
    fn drop(&mut self) {
        CHECKING.store(false, Ordering::SeqCst);
        notify_changed();
    }
}

fn check(trigger: Trigger) {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return;
    }
    notify_changed();
    let _guard = CheckingGuard;
    check_inner(trigger);
}

fn check_inner(trigger: Trigger) {
    let release = match fetch_latest() {
        Ok(release) => release,
        Err(error) => {
            eprintln!("AutoFxEmbed: update check failed: {error}");
            if trigger == Trigger::Manual {
                crate::dialog::notify(
                    "AutoFxEmbed: update check failed",
                    "Could not reach GitHub. Try again later.",
                );
            }
            return;
        }
    };
    {
        let mut state = state();
        state.last_check = now();
        save(&state);
    }

    let tag = release.tag_name.clone();
    if !is_newer(&tag, CURRENT) {
        if trigger == Trigger::Manual {
            crate::dialog::notify("AutoFxEmbed", &format!("You're up to date (v{CURRENT})."));
        }
        return;
    }
    if trigger == Trigger::Auto && state().skipped.as_deref() == Some(tag.as_str()) {
        return;
    }

    let exe = std::env::current_exe().ok();
    let title = "AutoFxEmbed update";
    if exe.as_deref().is_some_and(is_cargo_install) {
        crate::dialog::notify(
            title,
            &format!("Version {tag} is available. Update with:\n{CARGO_INSTALL_COMMAND}"),
        );
        return;
    }
    let target = asset_name()
        .and_then(|name| find_asset(&release, name))
        .and_then(|asset| {
            let hash = sha256_from_digest(asset.digest.as_deref()?)?;
            Some((asset, hash))
        });
    let (Some(exe), Some((asset, expected))) = (exe, target) else {
        crate::dialog::notify(
            title,
            &format!(
                "Version {tag} is available, but it can't be installed automatically.\n{}",
                release.html_url
            ),
        );
        return;
    };

    let question = format!(
        "Version {tag} is available (you have v{CURRENT}). Download and install it now?\n{}",
        release.html_url
    );
    if !crate::dialog::confirm(title, &question) {
        if trigger == Trigger::Auto {
            let mut state = state();
            state.skipped = Some(tag);
            save(&state);
        }
        return;
    }

    let failed = |reason: String| {
        eprintln!("AutoFxEmbed: update failed: {reason}");
        crate::dialog::notify(
            "AutoFxEmbed: update failed",
            &format!("{reason}\n{}", release.html_url),
        );
    };
    let bytes = match download(asset, expected) {
        Ok(bytes) => bytes,
        Err(reason) => return failed(reason),
    };
    if let Err(reason) = install(&bytes, &exe) {
        return failed(reason);
    }

    {
        let mut state = state();
        state.skipped = None;
        save(&state);
    }
    eprintln!("AutoFxEmbed: updated to {tag}; restarting");
    // `exe` was captured before the swap (on Linux the old path reads "(deleted)" after).
    let _ = RESTART.set(exe);
    crate::monitor::request_quit();
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "tag_name": "v0.2.0",
        "html_url": "https://github.com/crisbbc/AutoFXEmbed/releases/tag/v0.2.0",
        "draft": false,
        "assets": [
            {"name": "autofxembed-linux-x86_64", "size": 3330912,
             "browser_download_url": "https://example/linux",
             "digest": "sha256:50005014aa0464bd0402554eb9c7862088d87f2333d56a1ded6c15d417155471"},
            {"name": "autofxembed-windows-x86_64.exe", "size": 1748480,
             "browser_download_url": "https://example/windows",
             "digest": "sha256:24630d50e9587a29bbaddf8e463f3bb1e5a7de6c43812832afb0795401eb9f47"}
        ]
    }"#;

    #[test]
    fn finds_platform_assets_in_the_api_response() {
        let release: Release = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(release.tag_name, "v0.2.0");
        let linux = find_asset(&release, "autofxembed-linux-x86_64").unwrap();
        assert_eq!(linux.size, 3330912);
        assert!(sha256_from_digest(linux.digest.as_deref().unwrap()).is_some());
        let windows = find_asset(&release, "autofxembed-windows-x86_64.exe").unwrap();
        assert_eq!(windows.browser_download_url, "https://example/windows");
        assert!(find_asset(&release, "nope").is_none());
    }

    #[test]
    fn digests_must_be_sha256_hex() {
        let hex = "50005014aa0464bd0402554eb9c7862088d87f2333d56a1ded6c15d417155471";
        let parsed = sha256_from_digest(&format!("sha256:{hex}")).unwrap();
        assert_eq!(parsed[0], 0x50);
        assert_eq!(parsed[31], 0x71);
        assert_eq!(sha256_from_digest(hex), None);
        assert_eq!(sha256_from_digest(&format!("md5:{hex}")), None);
        assert_eq!(sha256_from_digest("sha256:abcd"), None);
        assert_eq!(
            sha256_from_digest(&format!("sha256:{}", "zz".repeat(32))),
            None
        );
    }

    #[test]
    fn auto_checks_respect_toggle_and_interval() {
        let day = CHECK_INTERVAL.as_secs();
        let mut state = State {
            auto_check: true,
            last_check: 1_000,
            skipped: None,
        };
        assert!(!auto_check_due(&state, 1_000 + day - 1));
        assert!(auto_check_due(&state, 1_000 + day));
        assert!(auto_check_due(&State::new(), day));
        state.auto_check = false;
        assert!(!auto_check_due(&state, 1_000 + 10 * day));
    }

    #[test]
    fn detects_cargo_installs() {
        assert!(is_cargo_install(Path::new(
            "/home/u/.cargo/bin/autofxembed"
        )));
        assert!(!is_cargo_install(Path::new(
            "/home/u/Downloads/autofxembed"
        )));
    }

    #[test]
    fn rejects_non_https_download_urls() {
        let asset = Asset {
            name: "autofxembed-linux-x86_64".into(),
            browser_download_url: "http://example.com/bin".into(),
            size: 100,
            digest: None,
        };
        assert!(download(&asset, [0u8; 32]).is_err());
    }

    #[test]
    fn state_defaults_fill_in_missing_fields() {
        let state: State = serde_json::from_str("{}").unwrap();
        assert_eq!(state, State::new());
    }
}
