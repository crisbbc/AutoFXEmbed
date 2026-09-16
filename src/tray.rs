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
    SetForegroundWindow, TrackPopupMenu, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, MB_ICONINFORMATION,
    MB_OK, MF_CHECKED, MF_DEFAULT, MF_DISABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_TOPALIGN, WM_NULL, WM_RBUTTONUP,
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
const MENU_FIXUP: u32 = 4;
#[cfg(target_os = "windows")]
const MENU_BOY: u32 = 5;

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

    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    // X has a configurable submenu; fixed and upcoming providers are informational.
    let x_submenu = CreatePopupMenu();
    if x_submenu.is_null() {
        DestroyMenu(menu);
        return;
    }
    let fixup_checked = if crate::config::is_boypussyx() {
        0
    } else {
        MF_CHECKED
    };
    let boy_checked = if crate::config::is_boypussyx() {
        MF_CHECKED
    } else {
        0
    };
    if AppendMenuW(
        x_submenu,
        MF_STRING | fixup_checked,
        MENU_FIXUP as usize,
        crate::clipboard::wide("FixUpX (fxtwitter / fixupx)").as_ptr(),
    ) == 0
        || AppendMenuW(
            x_submenu,
            MF_STRING | boy_checked,
            MENU_BOY as usize,
            crate::clipboard::wide("BoyPussyX (boypussyx.com)").as_ptr(),
        ) == 0
    {
        DestroyMenu(x_submenu);
        DestroyMenu(menu);
        return;
    }
    if AppendMenuW(
        menu,
        MF_STRING | MF_POPUP,
        x_submenu as usize,
        crate::clipboard::wide("X / Twitter \u{25B6}").as_ptr(),
    ) == 0
        || AppendMenuW(
            menu,
            MF_STRING | MF_GRAYED | MF_DISABLED,
            0,
            crate::clipboard::wide("Instagram (instagram7.com)").as_ptr(),
        ) == 0
        || AppendMenuW(
            menu,
            MF_STRING | MF_GRAYED | MF_DISABLED,
            0,
            crate::clipboard::wide("TikTok (tnktok.com)").as_ptr(),
        ) == 0
        || AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) == 0
    {
        DestroyMenu(menu);
        return;
    }

    // Start on startup (checkable — check reflects current registry state).
    let startup_flags = MF_STRING
        | if crate::autostart::is_enabled() {
            MF_CHECKED
        } else {
            0
        };
    if AppendMenuW(
        menu,
        startup_flags,
        MENU_STARTUP as usize,
        crate::clipboard::wide("Start on startup").as_ptr(),
    ) == 0
        || AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) == 0
    {
        DestroyMenu(menu);
        return;
    }

    // About (bold — MF_DEFAULT marks it as the default menu item).
    if AppendMenuW(
        menu,
        MF_STRING | MF_DEFAULT,
        MENU_ABOUT as usize,
        crate::clipboard::wide("About").as_ptr(),
    ) == 0
        || AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null()) == 0
    {
        DestroyMenu(menu);
        return;
    }

    // Quit.
    if AppendMenuW(
        menu,
        MF_STRING,
        MENU_QUIT as usize,
        crate::clipboard::wide("Quit").as_ptr(),
    ) == 0
    {
        DestroyMenu(menu);
        return;
    }

    // Win32 requires the owner window to be foreground for notification-area
    // menus to dismiss correctly when the user clicks elsewhere.
    if SetForegroundWindow(hwnd) == 0 {
        eprintln!("AutoFxEmbed: failed to foreground tray menu owner");
    }
    let cmd = TrackPopupMenu(
        menu,
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
    DestroyMenu(menu);

    match cmd as u32 {
        MENU_FIXUP => crate::config::set_boypussyx(false),
        MENU_BOY => crate::config::set_boypussyx(true),
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
        let is_boy = crate::config::is_boypussyx();
        vec![
            SubMenu {
                label: "X / Twitter".into(),
                submenu: vec![
                    StandardItem {
                        label: if !is_boy {
                            "✓ FixUpX (fxtwitter / fixupx)".into()
                        } else {
                            "  FixUpX (fxtwitter / fixupx)".into()
                        },
                        activate: Box::new(|_tray| {
                            crate::config::set_boypussyx(false);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    StandardItem {
                        label: if is_boy {
                            "✓ BoyPussyX (boypussyx.com)".into()
                        } else {
                            "  BoyPussyX (boypussyx.com)".into()
                        },
                        activate: Box::new(|_tray| {
                            crate::config::set_boypussyx(true);
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
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
                    if let Err(error) = std::process::Command::new("notify-send")
                        .args([
                            "--app-name=AutoFxEmbed",
                            "About AutoFxEmbed",
                            "Made by Cris with much <3 for his friends",
                        ])
                        .spawn()
                    {
                        eprintln!("AutoFxEmbed: unable to show About notification: {error}");
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

/// Spawn the D-Bus tray service and return the handle + quit flag.
#[cfg(target_os = "linux")]
pub fn spawn() -> Result<(Handle<LinuxTray>, Arc<AtomicBool>), ksni::Error> {
    let quit = Arc::new(AtomicBool::new(false));
    let tray = LinuxTray {
        quit_requested: quit.clone(),
    };
    let handle = tray.spawn()?;
    Ok((handle, quit))
}
