//! System tray icon with context menu.
//!
//! ## Windows
//!
//! Uses raw Win32 shell + user32 APIs to create a tray icon and popup menu.
//! Menu events arrive via the window message pump in [`crate::monitor`].
//!
//! ## Linux
//!
//! Uses the `tray-icon` crate (which wraps libayatana-appindicator / GTK).
//! Menu events are delivered through a channel that the monitor loop polls.

// ---------------------------------------------------------------------------
// Windows implementation (Win32)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IMAGE_ICON, LoadImageW,
    LR_DEFAULTSIZE, LR_SHARED, MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_DEFAULT, MF_SEPARATOR,
    MF_STRING, MessageBoxW, TrackPopupMenu, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_TOPALIGN, WM_RBUTTONUP,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

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

/// # Safety
/// Calls Win32 shell + user32 APIs.
#[cfg(target_os = "windows")]
pub unsafe fn add(hwnd: HWND) {
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
    let n = tip.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&tip[..n]);
    Shell_NotifyIconW(NIM_ADD, &nid);
}

/// # Safety
/// Calls Win32 shell API.
#[cfg(target_os = "windows")]
pub unsafe fn remove(hwnd: HWND) {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

/// Handle a tray callback message. `lparam`'s low word is the mouse event.
/// On right-click we show a popup menu with "Quit".
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

    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    // Start on startup (checkable — check reflects current registry state).
    let startup_flags = MF_STRING | if crate::autostart::is_enabled() { MF_CHECKED } else { 0 };
    AppendMenuW(
        menu,
        startup_flags,
        MENU_STARTUP as usize,
        crate::clipboard::wide("Start on startup").as_ptr(),
    );

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // About (bold — MF_DEFAULT marks it as the default menu item).
    AppendMenuW(
        menu,
        MF_STRING | MF_DEFAULT,
        MENU_ABOUT as usize,
        crate::clipboard::wide("About").as_ptr(),
    );

    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    // Quit.
    AppendMenuW(
        menu,
        MF_STRING,
        MENU_QUIT as usize,
        crate::clipboard::wide("Quit").as_ptr(),
    );

    let cmd = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    DestroyMenu(menu);

    match cmd as u32 {
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
    Arc,
};

#[cfg(target_os = "linux")]
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::{CheckmarkItem, MenuItem, StandardItem},
    Icon, Tray as KsniTray,
};

/// Tray data that implements the ksni [`KsniTray`] trait.
/// The D-Bus menu is rebuilt on every right-click via [`KsniTray::menu`],
/// so the "Start on startup" checkmark always reflects the live state.
#[cfg(target_os = "linux")]
pub struct LinuxTray {
    /// Set to `true` from the Quit menu callback; the monitor loop reads it.
    pub quit_requested: Arc<AtomicBool>,
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
        vec![
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
                    // Best-effort desktop notification (ignore failures).
                    let _ = std::process::Command::new("notify-send")
                        .args([
                            "--app-name=AutoFxEmbed",
                            "About AutoFxEmbed",
                            "Made by Cris with much <3 for his friends",
                        ])
                        .spawn();
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

/// Spawn the D-Bus tray service and return the handle + quit flag.
#[cfg(target_os = "linux")]
pub fn spawn() -> (Handle<LinuxTray>, Arc<AtomicBool>) {
    let quit = Arc::new(AtomicBool::new(false));
    let tray = LinuxTray {
        quit_requested: quit.clone(),
    };
    let handle = tray.spawn().expect("ksni tray should spawn on Linux");
    (handle, quit)
}
