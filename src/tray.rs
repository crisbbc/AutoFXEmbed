//! System tray icon with context menu.
//!
//! ## Windows
//!
//! Uses raw Win32 shell + user32 APIs to create a tray icon and popup menu.
//! Menu events arrive via the window message pump in [`crate::monitor`].
//!
//! ## Linux
//!
//! Uses `ksni` to expose a pure-Rust D-Bus StatusNotifierItem.
//! Menu callbacks update shared state read by the monitor loop.

// ---------------------------------------------------------------------------
// Windows implementation (Win32)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadImageW, MessageBoxW, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, HMENU, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
    MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_DEFAULT, MF_DISABLED, MF_GRAYED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_TOPALIGN, WM_NULL,
    WM_RBUTTONUP,
};

/// Custom message Windows sends to our window when the tray icon is interacted with.
/// (WM_APP = 0x8000.)
#[cfg(target_os = "windows")]
pub const TRAY_CALLBACK_MSG: u32 = 0x8001;

/// Menu item IDs (Windows).
#[cfg(target_os = "windows")]
const MENU_QUIT: u32 = 1;
#[cfg(target_os = "windows")]
const MENU_STARTUP: u32 = 2;
#[cfg(target_os = "windows")]
const MENU_ABOUT: u32 = 3;
#[cfg(target_os = "windows")]
const MENU_CLEAR_HISTORY: u32 = 4;
/// First ID of the Recent items (`MENU_RECENT_BASE + index in the snapshot`).
#[cfg(target_os = "windows")]
const MENU_RECENT_BASE: u32 = 200;
/// First ID of the X / Twitter target items (`MENU_X_BASE + index in XTarget::ALL`).
#[cfg(target_os = "windows")]
const MENU_X_BASE: u32 = 100;

/// Owned popup menu handle; destroyed on drop (which also frees any submenu
/// that was successfully attached to it).
#[cfg(target_os = "windows")]
struct Menu(HMENU);

#[cfg(target_os = "windows")]
impl Menu {
    unsafe fn popup() -> Option<Menu> {
        let handle = CreatePopupMenu();
        (!handle.is_null()).then(|| Menu(handle))
    }

    unsafe fn append(&self, flags: u32, id: usize, text: &str) -> bool {
        let text = crate::clipboard::wide(text);
        AppendMenuW(self.0, flags, id, text.as_ptr()) != 0
    }

    unsafe fn separator(&self) -> bool {
        AppendMenuW(self.0, MF_SEPARATOR, 0, std::ptr::null()) != 0
    }

    /// Attach `sub` as a popup. On success `self` owns it; on failure `sub`
    /// is dropped (destroyed) here.
    unsafe fn append_submenu(&self, sub: Menu, text: &str) -> bool {
        if self.append(MF_STRING | MF_POPUP, sub.0 as usize, text) {
            std::mem::forget(sub);
            true
        } else {
            false
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.0);
        }
    }
}

/// # Safety
/// Calls Win32 shell + user32 APIs.
#[cfg(target_os = "windows")]
pub unsafe fn add(hwnd: HWND) -> bool {
    let tip = crate::clipboard::wide("AutoFxEmbed");
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = TRAY_CALLBACK_MSG;
    let hinst = GetModuleHandleW(std::ptr::null());
    nid.hIcon = LoadImageW(
        hinst,
        1u16 as usize as windows_sys::core::PCWSTR,
        IMAGE_ICON,
        0,
        0,
        LR_DEFAULTSIZE | LR_SHARED,
    );
    if nid.hIcon.is_null() {
        eprintln!("AutoFxEmbed: failed to load tray icon");
        return false;
    }
    let n = tip.len().min(nid.szTip.len().saturating_sub(1));
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    Shell_NotifyIconW(NIM_ADD, &nid) != 0
}

/// # Safety
/// Calls Win32 shell API.
#[cfg(target_os = "windows")]
pub unsafe fn remove(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    if Shell_NotifyIconW(NIM_DELETE, &nid) == 0 {
        eprintln!("AutoFxEmbed: failed to remove tray icon");
    }
}

/// Handle a tray callback message. `lparam`'s low word is the mouse event.
/// On right-click we show the startup, About, and Quit menu items.
///
/// # Safety
/// Calls Win32 menu/user32 APIs.
#[cfg(target_os = "windows")]
pub unsafe fn handle_event(hwnd: HWND, lparam: LPARAM) {
    let mouse_msg = (lparam as u32) & 0xFFFF;
    if mouse_msg != WM_RBUTTONUP {
        return;
    }

    let mut pt: POINT = std::mem::zeroed();
    if GetCursorPos(&mut pt) == 0 {
        return;
    }

    let Some(menu) = Menu::popup() else {
        return;
    };

    // X has a configurable submenu; fixed and upcoming providers are informational.
    let Some(x_submenu) = Menu::popup() else {
        return;
    };
    let x_target = crate::config::x_target();
    for (index, target) in crate::config::XTarget::ALL.into_iter().enumerate() {
        let checked = if x_target == target { MF_CHECKED } else { 0 };
        if !x_submenu.append(
            MF_STRING | checked,
            (MENU_X_BASE as usize) + index,
            target.label(),
        ) {
            return;
        }
    }
    if !menu.append_submenu(x_submenu, "X / Twitter") {
        return;
    }
    if !menu.append(
        MF_STRING | MF_GRAYED | MF_DISABLED,
        0,
        "Instagram (instagram7.com)",
    ) || !menu.append(
        MF_STRING | MF_GRAYED | MF_DISABLED,
        0,
        "TikTok (tnktok.com)",
    ) || !menu.separator()
    {
        return;
    }

    // Recent conversions. The snapshot is kept so a returned ID maps to the
    // entry that was shown, even if the history changes while the menu is open.
    let recent = crate::history::recent(crate::history::MENU_ENTRIES);
    let Some(recent_submenu) = Menu::popup() else {
        return;
    };
    if recent.is_empty()
        && !recent_submenu.append(MF_STRING | MF_GRAYED | MF_DISABLED, 0, "(empty)")
    {
        return;
    }
    for (index, entry) in recent.iter().enumerate() {
        // `&` marks a mnemonic in Win32 menus; double it to show it literally.
        let label = crate::history::menu_label(entry).replace('&', "&&");
        if !recent_submenu.append(MF_STRING, (MENU_RECENT_BASE as usize) + index, &label) {
            return;
        }
    }
    if !menu.append_submenu(recent_submenu, "Recent")
        || !menu.append(MF_STRING, MENU_CLEAR_HISTORY as usize, "Clear history")
        || !menu.separator()
    {
        return;
    }

    // Start on startup (checkable — check reflects current registry state).
    let startup_flags = MF_STRING
        | if crate::autostart::is_enabled() {
            MF_CHECKED
        } else {
            0
        };
    if !menu.append(startup_flags, MENU_STARTUP as usize, "Start on startup") || !menu.separator() {
        return;
    }

    // About (bold — MF_DEFAULT marks it as the default menu item).
    if !menu.append(MF_STRING | MF_DEFAULT, MENU_ABOUT as usize, "About") || !menu.separator() {
        return;
    }

    // Quit.
    if !menu.append(MF_STRING, MENU_QUIT as usize, "Quit") {
        return;
    }

    // Win32 requires the owner window to be foreground for notification-area
    // menus to dismiss correctly when the user clicks elsewhere.
    if SetForegroundWindow(hwnd) == 0 {
        eprintln!("AutoFxEmbed: failed to foreground tray menu owner");
    }
    let cmd = TrackPopupMenu(
        menu.0,
        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    if PostMessageW(hwnd, WM_NULL, 0, 0) == 0 {
        eprintln!("AutoFxEmbed: failed to finalize tray menu");
    }
    drop(menu);

    match cmd as u32 {
        c if (MENU_X_BASE..MENU_X_BASE + crate::config::XTarget::ALL.len() as u32).contains(&c) => {
            crate::config::set_x_target(crate::config::XTarget::ALL[(c - MENU_X_BASE) as usize]);
        }
        c if (MENU_RECENT_BASE..MENU_RECENT_BASE + recent.len() as u32).contains(&c) => {
            let entry = &recent[(c - MENU_RECENT_BASE) as usize];
            if !crate::clipboard::write_text(&entry.embed) {
                eprintln!("AutoFxEmbed: unable to copy history entry");
            }
        }
        MENU_CLEAR_HISTORY => {
            crate::history::clear();
        }
        MENU_STARTUP => {
            crate::autostart::toggle();
        }
        MENU_ABOUT => {
            MessageBoxW(
                hwnd,
                crate::clipboard::wide("Made by Cris with much <3 for his friends").as_ptr(),
                crate::clipboard::wide("About").as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        MENU_QUIT => {
            windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Linux implementation (ksni / D-Bus StatusNotifierItem)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[cfg(target_os = "linux")]
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::{CheckmarkItem, MenuItem, StandardItem, SubMenu},
    Icon, Tray as KsniTray,
};

/// Tray data that implements the ksni [`KsniTray`] trait.
/// The D-Bus menu is rebuilt on every right-click via [`KsniTray::menu`],
/// so the "Start on startup" checkmark always reflects the live state.
#[cfg(target_os = "linux")]
pub struct LinuxTray {
    /// Set to `true` from the Quit menu callback; the monitor loop reads it.
    pub quit_requested: Arc<AtomicBool>,
    /// Link picked from the Recent submenu; the monitor loop owns the
    /// clipboard (on Wayland it must stay alive to serve pastes) and copies it.
    pub copy_request: Arc<Mutex<Option<String>>>,
}

#[cfg(target_os = "linux")]
impl KsniTray for LinuxTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "AutoFxEmbed".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        // Icon is pre-converted from assets/fxembed.ico by build.rs.
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/icon.argb"));
        if bytes.len() < 8 {
            return vec![];
        }
        let w = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i32;
        let h = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as i32;
        vec![Icon {
            width: w,
            height: h,
            data: bytes[8..].to_vec(),
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "AutoFxEmbed".into(),
            description: "Auto-rewrite social-media links in the clipboard".into(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let quit = self.quit_requested.clone();
        let copy_request = self.copy_request.clone();
        let x_target = crate::config::x_target();
        let x_item = |target: crate::config::XTarget| -> MenuItem<Self> {
            let mark = if x_target == target { "✓" } else { " " };
            StandardItem {
                label: format!("{mark} {}", target.label()),
                activate: Box::new(move |_tray| {
                    crate::config::set_x_target(target);
                }),
                ..Default::default()
            }
            .into()
        };
        vec![
            SubMenu {
                label: "X / Twitter".into(),
                submenu: crate::config::XTarget::ALL
                    .into_iter()
                    .map(x_item)
                    .collect(),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Instagram (instagram7.com)".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "TikTok (tnktok.com)".into(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Recent".into(),
                submenu: recent_items(copy_request),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Clear history".into(),
                activate: Box::new(|_tray| {
                    crate::history::clear();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Start on startup".into(),
                checked: crate::autostart::is_enabled(),
                activate: Box::new(|_tray| {
                    crate::autostart::toggle();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "About".into(),
                activate: Box::new(|_tray| {
                    // Best-effort desktop notification (ignore failures). The child
                    // is reaped on a helper thread so it never lingers as a zombie.
                    match std::process::Command::new("notify-send")
                        .args([
                            "--app-name=AutoFxEmbed",
                            "About AutoFxEmbed",
                            "Made by Cris with much <3 for his friends",
                        ])
                        .spawn()
                    {
                        Ok(mut child) => {
                            std::thread::spawn(move || {
                                let _ = child.wait();
                            });
                        }
                        Err(error) => {
                            eprintln!("AutoFxEmbed: unable to show About notification: {error}");
                        }
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_tray| {
                    quit.store(true, Ordering::SeqCst);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Items of the Recent submenu: the newest conversions, each copying its embed
/// link back to the clipboard when clicked.
#[cfg(target_os = "linux")]
fn recent_items(copy_request: Arc<Mutex<Option<String>>>) -> Vec<MenuItem<LinuxTray>> {
    let recent = crate::history::recent(crate::history::MENU_ENTRIES);
    if recent.is_empty() {
        return vec![StandardItem {
            label: "(empty)".into(),
            enabled: false,
            ..Default::default()
        }
        .into()];
    }
    recent
        .into_iter()
        .map(|entry| {
            let copy_request = copy_request.clone();
            StandardItem {
                // `_` marks a mnemonic in DBusMenu labels; double it to show it literally.
                label: crate::history::menu_label(&entry).replace('_', "__"),
                activate: Box::new(move |_tray| {
                    if let Ok(mut request) = copy_request.lock() {
                        *request = Some(entry.embed.clone());
                    }
                }),
                ..Default::default()
            }
            .into()
        })
        .collect()
}

/// What [`spawn`] hands back to the monitor loop.
#[cfg(target_os = "linux")]
pub struct TrayControl {
    pub handle: Handle<LinuxTray>,
    pub quit: Arc<AtomicBool>,
    /// Embed link the user picked from the Recent submenu, waiting to be copied.
    pub copy_request: Arc<Mutex<Option<String>>>,
}

/// Spawn the D-Bus tray service.
#[cfg(target_os = "linux")]
pub fn spawn() -> Result<TrayControl, ksni::Error> {
    let quit = Arc::new(AtomicBool::new(false));
    let copy_request = Arc::new(Mutex::new(None));
    let tray = LinuxTray {
        quit_requested: quit.clone(),
        copy_request: copy_request.clone(),
    };
    let handle = tray.spawn()?;

    // Re-render the menu whenever the history changes (new conversion, fetched
    // metadata, clear). The update runs on its own thread: `clear` is invoked
    // from inside a tray callback, where a blocking update would deadlock.
    let refresh = handle.clone();
    crate::history::set_on_change(move || {
        let refresh = refresh.clone();
        std::thread::spawn(move || {
            refresh.update(|_| {});
        });
    });

    Ok(TrayControl {
        handle,
        quit,
        copy_request,
    })
}
