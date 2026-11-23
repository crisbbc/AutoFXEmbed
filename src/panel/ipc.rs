//! Messages exchanged with the flyout process, and the parent-side logic that
//! spawns it, pushes live state updates, and applies its actions.
//!
//! The panel is this same binary started with `--panel`: the parent writes JSON
//! `PanelState` lines to its stdin (initial state plus live updates), and the
//! panel answers with one JSON `Action` line on stdout per user action.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{autostart, clipboard, config, history, update};

/// Clicking the tray icon blurs (and so closes) the panel just before the click
/// arrives; a panel that closed this recently must not be reopened by it.
const REOPEN_GUARD: Duration = Duration::from_millis(400);

/// Everything the panel needs to draw itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelState {
    pub about: String,
    /// Tray icon position in screen pixels, when known.
    pub anchor: Option<(f32, f32)>,
    /// Monitor work area `(left, top, right, bottom)` in screen pixels, when known.
    #[serde(default)]
    pub work_area: Option<(f32, f32, f32, f32)>,
    #[serde(default)]
    pub paused: bool,
    /// Built-in X / Twitter targets as `(id, label)`.
    pub targets: Vec<(String, String)>,
    pub selected_builtin: Option<String>,
    pub selected_custom: Option<String>,
    pub customs: Vec<String>,
    #[serde(default = "yes")]
    pub rewrite_x: bool,
    #[serde(default = "yes")]
    pub rewrite_bsky: bool,
    #[serde(default = "yes")]
    pub rewrite_tiktok: bool,
    #[serde(default)]
    pub x_translate_lang: Option<String>,
    #[serde(default)]
    pub extra_rules: Vec<(String, String)>,
    pub recent: Vec<history::Entry>,
    pub startup: bool,
    pub auto_update: bool,
    pub strip_trackers: bool,
    #[serde(default)]
    pub checking_update: bool,
}

fn yes() -> bool {
    true
}

impl PanelState {
    fn capture(anchor: Option<(f32, f32)>, work_area: Option<(f32, f32, f32, f32)>) -> Self {
        let (selected_builtin, selected_custom) = match config::selection() {
            config::Selection::Builtin(target) => (Some(target.id().to_string()), None),
            config::Selection::Custom(domain) => (None, Some(domain)),
        };
        Self {
            about: crate::tray::about_text(),
            anchor,
            work_area,
            paused: config::is_paused(),
            targets: config::XTarget::ALL
                .iter()
                .map(|t| (t.id().to_string(), t.label().to_string()))
                .collect(),
            selected_builtin,
            selected_custom,
            customs: config::custom_domains(),
            rewrite_x: config::rewrite_x(),
            rewrite_bsky: config::rewrite_bsky(),
            rewrite_tiktok: config::rewrite_tiktok(),
            x_translate_lang: config::x_translate_lang(),
            extra_rules: config::extra_rules(),
            recent: history::all(),
            startup: autostart::is_enabled(),
            auto_update: update::auto_enabled(),
            strip_trackers: config::strip_trackers(),
            checking_update: update::is_checking(),
        }
    }
}

/// What the user did in the panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    TogglePaused,
    SelectBuiltin(String),
    SelectCustom(String),
    AddCustom(String),
    RemoveCustom(String),
    ToggleSiteX,
    ToggleSiteBsky,
    ToggleSiteTiktok,
    SetXTranslate(String),
    AddExtraRule(String, String),
    RemoveExtraRule(String),
    CopyRecent(String),
    CopyOriginal(String),
    RemoveRecent(String),
    ClearHistory,
    ToggleTrackers,
    ToggleStartup,
    ToggleAutoUpdate,
    CheckUpdate,
    Quit,
}

type Handler<T> = Box<dyn Fn(T) + Send + Sync>;
static COPY_HANDLER: OnceLock<Handler<String>> = OnceLock::new();
static QUIT_HANDLER: OnceLock<Handler<()>> = OnceLock::new();

/// Override how the parent copies a link and quits. The Linux tray needs this
/// because the monitor loop must own the clipboard (Wayland) and event loop;
/// Windows falls back to the defaults.
pub fn set_handlers(
    copy: impl Fn(String) + Send + Sync + 'static,
    quit: impl Fn() + Send + Sync + 'static,
) {
    let _ = COPY_HANDLER.set(Box::new(copy));
    let _ = QUIT_HANDLER.set(Box::new(move |()| quit()));
}

fn dispatch(action: Action) {
    match action {
        Action::TogglePaused => {
            config::toggle_paused();
        }
        Action::SelectBuiltin(id) => {
            if let Some(target) = config::XTarget::ALL.iter().find(|t| t.id() == id) {
                config::select_builtin(*target);
            }
        }
        Action::SelectCustom(domain) => config::select_custom(&domain),
        Action::AddCustom(input) => {
            if let Err(reason) = config::add_custom(&input) {
                eprintln!("AutoFxEmbed: domain not added: {reason}");
            }
        }
        Action::RemoveCustom(domain) => config::remove_custom(&domain),
        Action::ToggleSiteX => config::toggle_rewrite_x(),
        Action::ToggleSiteBsky => config::toggle_rewrite_bsky(),
        Action::ToggleSiteTiktok => config::toggle_rewrite_tiktok(),
        Action::SetXTranslate(lang) => {
            if let Err(reason) = config::set_x_translate_lang(&lang) {
                eprintln!("AutoFxEmbed: translate lang not set: {reason}");
            }
        }
        Action::AddExtraRule(source, target) => {
            if let Err(reason) = config::add_extra_rule(&source, &target) {
                eprintln!("AutoFxEmbed: site rule not added: {reason}");
            }
        }
        Action::RemoveExtraRule(source) => config::remove_extra_rule(&source),
        Action::CopyRecent(embed) => match COPY_HANDLER.get() {
            Some(copy) => copy(embed),
            None => {
                if !clipboard::write_text(&embed) {
                    eprintln!("AutoFxEmbed: unable to copy history entry");
                }
            }
        },
        Action::CopyOriginal(original) => match COPY_HANDLER.get() {
            Some(copy) => copy(original),
            None => {
                #[cfg(target_os = "windows")]
                let ok = crate::monitor::copy_without_rewrite(&original);
                #[cfg(not(target_os = "windows"))]
                let ok = clipboard::write_text(&original);
                if !ok {
                    eprintln!("AutoFxEmbed: unable to copy original link");
                }
            }
        },
        Action::RemoveRecent(embed) => history::remove(&embed),
        Action::ClearHistory => history::clear(),
        Action::ToggleTrackers => config::toggle_strip_trackers(),
        Action::ToggleStartup => {
            autostart::toggle();
            push_state();
        }
        Action::ToggleAutoUpdate => update::toggle_auto(),
        Action::CheckUpdate => update::check_now(),
        Action::Quit => match QUIT_HANDLER.get() {
            Some(quit) => quit(()),
            None => crate::monitor::request_quit(),
        },
    }
}

struct ActivePanel {
    id: u64,
    child: Child,
    stdin: ChildStdin,
    anchor: Option<(f32, f32)>,
    work_area: Option<(f32, f32, f32, f32)>,
}

static PANEL: Mutex<Option<ActivePanel>> = Mutex::new(None);
static LAST_CLOSED: Mutex<Option<Instant>> = Mutex::new(None);
static NEXT_PANEL_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn lock<T>(mutex: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Push a fresh `PanelState` snapshot to the currently open panel, if any.
pub fn push_state() {
    let mut panel = lock(&PANEL);
    let Some(active) = panel.as_mut() else {
        return;
    };
    let state = PanelState::capture(active.anchor, active.work_area);
    if let Ok(line) = serde_json::to_string(&state) {
        if writeln!(active.stdin, "{line}").is_err() || active.stdin.flush().is_err() {
            // Stdin pipe closed; panel is shutting down.
            let _ = active.child.try_wait();
        }
    }
}

/// Open the panel near `anchor`, or close it if it is already showing.
/// Returns immediately.
pub fn open_or_toggle(anchor: Option<(f32, f32)>, work_area: Option<(f32, f32, f32, f32)>) {
    let mut panel = lock(&PANEL);
    if let Some(active) = panel.as_mut() {
        if matches!(active.child.try_wait(), Ok(None)) {
            let _ = active.child.kill();
            let _ = active.child.wait();
            *panel = None;
            return;
        }
        *panel = None;
    }
    if lock(&LAST_CLOSED).is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
        return;
    }
    drop(panel);

    // Reload hand-edited files and retry any previously offline metadata fetches.
    config::reload_custom();
    history::retry_pending();

    match spawn(anchor, work_area) {
        Ok(active) => *lock(&PANEL) = Some(active),
        Err(error) => eprintln!("AutoFxEmbed: unable to open the panel: {error}"),
    }
}

fn spawn(
    anchor: Option<(f32, f32)>,
    work_area: Option<(f32, f32, f32, f32)>,
) -> std::io::Result<ActivePanel> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--panel");
    // Wayland clients cannot choose their window position. Run the panel
    // through XWayland (when available) so it can open next to the tray icon.
    if cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_some()
        && std::env::var_os("AUTOFXEMBED_NATIVE_WAYLAND").is_none()
    {
        command.env_remove("WAYLAND_DISPLAY");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("panel stdin unavailable"))?;
    let state = PanelState::capture(anchor, work_area);
    let line = serde_json::to_string(&state).map_err(std::io::Error::other)?;
    writeln!(stdin, "{line}")?;
    stdin.flush()?;

    let id = NEXT_PANEL_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str::<Action>(&line) {
                    Ok(action) => dispatch(action),
                    Err(error) => eprintln!("AutoFxEmbed: bad panel message: {error}"),
                }
            }
            // Reap the exited child immediately so no zombie lingers until next click.
            let mut panel = lock(&PANEL);
            if panel.as_ref().is_some_and(|p| p.id == id) {
                if let Some(mut active) = panel.take() {
                    let _ = active.child.wait();
                }
            }
            *lock(&LAST_CLOSED) = Some(Instant::now());
        });
    }
    Ok(ActivePanel {
        id,
        child,
        stdin,
        anchor,
        work_area,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_and_panel_state_round_trip_json() {
        let actions = [
            Action::TogglePaused,
            Action::ToggleSiteX,
            Action::ToggleSiteBsky,
            Action::ToggleSiteTiktok,
            Action::SetXTranslate("en".into()),
            Action::AddExtraRule("reddit.com".into(), "rxddit.com".into()),
            Action::RemoveExtraRule("reddit.com".into()),
            Action::CopyOriginal("https://x.com/u/status/1".into()),
            Action::RemoveRecent("https://fixupx.com/u/status/1".into()),
        ];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let _: Action = serde_json::from_str(&json).unwrap();
        }
    }
}
