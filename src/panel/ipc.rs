//! Messages exchanged with the flyout process, and the parent-side logic that
//! spawns it and applies its actions.
//!
//! The panel is this same binary started with `--panel`: the parent writes one
//! JSON `PanelState` line to its stdin, and the panel answers with one JSON
//! `Action` line on stdout per user action.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
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
    /// Built-in X / Twitter targets as `(id, label)`.
    pub targets: Vec<(String, String)>,
    pub selected_builtin: Option<String>,
    pub selected_custom: Option<String>,
    pub customs: Vec<String>,
    pub recent: Vec<history::Entry>,
    pub startup: bool,
    pub auto_update: bool,
}

impl PanelState {
    fn capture(anchor: Option<(f32, f32)>) -> Self {
        config::reload_custom();
        let (selected_builtin, selected_custom) = match config::selection() {
            config::Selection::Builtin(target) => (Some(target.id().to_string()), None),
            config::Selection::Custom(domain) => (None, Some(domain)),
        };
        Self {
            about: crate::tray::about_text(),
            anchor,
            targets: config::XTarget::ALL
                .iter()
                .map(|t| (t.id().to_string(), t.label().to_string()))
                .collect(),
            selected_builtin,
            selected_custom,
            customs: config::custom_domains(),
            recent: history::recent(history::MENU_ENTRIES),
            startup: autostart::is_enabled(),
            auto_update: update::auto_enabled(),
        }
    }
}

/// What the user did in the panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    SelectBuiltin(String),
    SelectCustom(String),
    AddCustom(String),
    RemoveCustom(String),
    CopyRecent(String),
    ClearHistory,
    ToggleStartup,
    ToggleAutoUpdate,
    CheckUpdate,
    Quit,
}

type Handler<T> = Box<dyn Fn(T) + Send + Sync>;
static COPY_HANDLER: OnceLock<Handler<String>> = OnceLock::new();
static QUIT_HANDLER: OnceLock<Handler<()>> = OnceLock::new();

/// Override how the parent copies a history link and quits. The Linux tray
/// needs this because the monitor loop must own the clipboard (Wayland) and
/// the exit flag; Windows falls back to the defaults.
pub fn set_handlers(
    copy: impl Fn(String) + Send + Sync + 'static,
    quit: impl Fn() + Send + Sync + 'static,
) {
    let _ = COPY_HANDLER.set(Box::new(copy));
    let _ = QUIT_HANDLER.set(Box::new(move |()| quit()));
}

fn dispatch(action: Action) {
    match action {
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
        Action::CopyRecent(embed) => match COPY_HANDLER.get() {
            Some(copy) => copy(embed),
            None => {
                if !clipboard::write_text(&embed) {
                    eprintln!("AutoFxEmbed: unable to copy history entry");
                }
            }
        },
        Action::ClearHistory => history::clear(),
        Action::ToggleStartup => {
            autostart::toggle();
        }
        Action::ToggleAutoUpdate => update::toggle_auto(),
        Action::CheckUpdate => update::check_now(),
        Action::Quit => match QUIT_HANDLER.get() {
            Some(quit) => quit(()),
            None => crate::monitor::request_quit(),
        },
    }
}

static PANEL: Mutex<Option<Child>> = Mutex::new(None);
static LAST_CLOSED: Mutex<Option<Instant>> = Mutex::new(None);

fn lock<T>(mutex: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Open the panel near `anchor`, or close it if it is already showing.
/// Returns immediately.
pub fn open_or_toggle(anchor: Option<(f32, f32)>) {
    let mut panel = lock(&PANEL);
    if let Some(child) = panel.as_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
            *panel = None;
            return;
        }
    }
    if lock(&LAST_CLOSED).is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
        return;
    }
    match spawn(anchor) {
        Ok(child) => *panel = Some(child),
        Err(error) => eprintln!("AutoFxEmbed: unable to open the panel: {error}"),
    }
}

fn spawn(anchor: Option<(f32, f32)>) -> std::io::Result<Child> {
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

    let state = PanelState::capture(anchor);
    if let Some(mut stdin) = child.stdin.take() {
        let line = serde_json::to_string(&state).map_err(std::io::Error::other)?;
        writeln!(stdin, "{line}")?;
    }
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                match serde_json::from_str::<Action>(&line) {
                    Ok(action) => dispatch(action),
                    Err(error) => eprintln!("AutoFxEmbed: bad panel message: {error}"),
                }
            }
            *lock(&LAST_CLOSED) = Some(Instant::now());
        });
    }
    Ok(child)
}
