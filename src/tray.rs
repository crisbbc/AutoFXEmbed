//! System tray icon.
//!
//! Left-clicking the icon opens the flyout panel (see [`crate::panel`]);
//! right-clicking shows a minimal Open / Pause / Quit menu.
//!
//! ## Windows
//!
//! Uses raw Win32 shell + user32 APIs to create a tray icon and popup menu.
//! Tray events arrive via the window message pump in [`crate::monitor`].
//!
//! ## Linux
//!
//! Uses `ksni` to expose a pure-Rust D-Bus StatusNotifierItem. Activation
//! (left-click) opens the panel; the menu offers Open, Pause/Resume, and Quit.

/// Text of the About message: the running version plus the credit line.
pub(crate) fn about_text() -> String {
    format!(
        "AutoFxEmbed v{}\nMade by Cris with much <3 for his friends",
        env!("CARGO_PKG_VERSION")
    )
}

/// Refresh the tray tooltip/icon/menu (e.g. after toggling Pause).
pub fn refresh() {
    #[cfg(target_os = "windows")]
    refresh_windows();
    #[cfg(target_os = "linux")]
    refresh_linux();
}

// ---------------------------------------------------------------------------
// Windows implementation (Win32)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadImageW, PostMessageW,
    SetForegroundWindow, TrackPopupMenu, HMENU, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, MF_DEFAULT,
    MF_SEPARATOR, MF_STRING, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_TOPALIGN,
    WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
};

/// Custom message Windows sends to our window when the tray icon is interacted
/// with. (WM_APP = 0x8000.)
#[cfg(target_os = "windows")]
pub const TRAY_CALLBACK_MSG: u32 = 0x8001;

/// Menu item IDs (Windows).
#[cfg(target_os = "windows")]
const MENU_OPEN: u32 = 1;
#[cfg(target_os = "windows")]
const MENU_PAUSE: u32 = 2;
#[cfg(target_os = "windows")]
const MENU_QUIT: u32 = 3;

#[cfg(target_os = "windows")]
static TRAY_HWND: AtomicUsize = AtomicUsize::new(0);

/// Owned popup menu handle; destroyed on drop.
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
}

#[cfg(target_os = "windows")]
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.0);
        }
    }
}

#[cfg(target_os = "windows")]
fn tooltip_text() -> &'static str {
    if crate::config::is_paused() {
        "AutoFxEmbed (Paused)"
    } else {
        "AutoFxEmbed"
    }
}

/// # Safety
/// Calls Win32 shell + user32 APIs.
#[cfg(target_os = "windows")]
pub unsafe fn add(hwnd: HWND) -> bool {
    TRAY_HWND.store(hwnd as usize, Ordering::Relaxed);
    let tip = crate::clipboard::wide(tooltip_text());
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

#[cfg(target_os = "windows")]
fn refresh_windows() {
    let hwnd = TRAY_HWND.load(Ordering::Relaxed) as HWND;
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let tip = crate::clipboard::wide(tooltip_text());
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_TIP;
        let n = tip.len().min(nid.szTip.len().saturating_sub(1));
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// # Safety
/// Calls Win32 shell API.
#[cfg(target_os = "windows")]
pub unsafe fn remove(hwnd: HWND) {
    TRAY_HWND.store(0, Ordering::Relaxed);
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    if Shell_NotifyIconW(NIM_DELETE, &nid) == 0 {
        eprintln!("AutoFxEmbed: failed to remove tray icon");
    }
}

/// Work area `(left, top, right, bottom)` in screen pixels of the monitor
/// containing `pt`, so the flyout stays on the clicked monitor in multi-monitor setups.
#[cfg(target_os = "windows")]
unsafe fn work_area_for_point(pt: POINT) -> Option<(f32, f32, f32, f32)> {
    let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
    if monitor.is_null() {
        return None;
    }
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info) == 0 {
        return None;
    }
    let r = info.rcWork;
    Some((r.left as f32, r.top as f32, r.right as f32, r.bottom as f32))
}

/// Handle a tray callback message. `lparam`'s low word is the mouse event.
/// Left-click toggles the panel; right-click shows the Open / Pause / Quit menu.
///
/// # Safety
/// Calls Win32 menu/user32 APIs.
#[cfg(target_os = "windows")]
pub unsafe fn handle_event(hwnd: HWND, lparam: LPARAM) {
    let mouse_msg = (lparam as u32) & 0xFFFF;
    if mouse_msg != WM_LBUTTONUP && mouse_msg != WM_RBUTTONUP {
        return;
    }

    let mut pt: POINT = std::mem::zeroed();
    if GetCursorPos(&mut pt) == 0 {
        return;
    }
    let anchor = Some((pt.x as f32, pt.y as f32));
    let work_area = work_area_for_point(pt);
    if mouse_msg == WM_LBUTTONUP {
        crate::panel::open_or_toggle(anchor, work_area);
        return;
    }

    let Some(menu) = Menu::popup() else {
        return;
    };
    let pause_label = if crate::config::is_paused() {
        "Resume"
    } else {
        "Pause"
    };
    if !menu.append(MF_STRING | MF_DEFAULT, MENU_OPEN as usize, "Open")
        || !menu.append(MF_STRING, MENU_PAUSE as usize, pause_label)
        || !menu.separator()
        || !menu.append(MF_STRING, MENU_QUIT as usize, "Quit")
    {
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
        MENU_OPEN => crate::panel::open_or_toggle(anchor, work_area),
        MENU_PAUSE => {
            crate::config::toggle_paused();
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
use std::sync::{Arc, OnceLock};

#[cfg(target_os = "linux")]
use ksni::{
    blocking::{Handle, TrayMethods},
    menu::{MenuItem, StandardItem},
    Icon, Tray as KsniTray,
};

#[cfg(target_os = "linux")]
static TRAY_HANDLE: OnceLock<Handle<LinuxTray>> = OnceLock::new();

#[cfg(target_os = "linux")]
fn refresh_linux() {
    if let Some(handle) = TRAY_HANDLE.get() {
        let _ = handle.update(|_| {});
    }
}

/// Tray data that implements the ksni [`KsniTray`] trait.
#[cfg(target_os = "linux")]
pub struct LinuxTray {
    quit: Arc<dyn Fn() + Send + Sync>,
}

#[cfg(target_os = "linux")]
impl KsniTray for LinuxTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        if crate::config::is_paused() {
            "AutoFxEmbed (Paused)".into()
        } else {
            "AutoFxEmbed".into()
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        // Icon is pre-converted from assets/fxembed.ico by build.rs.
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/icon.argb"));
        if bytes.len() < 8 {
            return vec![];
        }
        let w = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i32;
        let h = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as i32;
        let mut data = bytes[8..].to_vec();
        // Dim & desaturate the tray icon when paused so status is obvious at a glance.
        if crate::config::is_paused() {
            for px in data.as_chunks_mut::<4>().0 {
                let gray =
                    ((px[1] as u16 * 77 + px[2] as u16 * 150 + px[3] as u16 * 29) >> 8) as u8;
                px[0] = (px[0] as u16 * 140 / 255) as u8;
                px[1] = gray;
                px[2] = gray;
                px[3] = gray;
            }
        }
        vec![Icon {
            width: w,
            height: h,
            data,
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let paused = crate::config::is_paused();
        ksni::ToolTip {
            title: if paused {
                "AutoFxEmbed (Paused)".into()
            } else {
                "AutoFxEmbed".into()
            },
            description: if paused {
                "Clipboard rewriting is paused".into()
            } else {
                "Auto-rewrite social-media links in the clipboard".into()
            },
            ..Default::default()
        }
    }

    /// Left-click: ksni passes the click position in screen coordinates.
    fn activate(&mut self, x: i32, y: i32) {
        crate::panel::open_or_toggle(Some((x as f32, y as f32)), None);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let quit = Arc::clone(&self.quit);
        let pause_label = if crate::config::is_paused() {
            "Resume"
        } else {
            "Pause"
        };
        vec![
            StandardItem {
                label: "Open".into(),
                activate: Box::new(|_tray| crate::panel::open_or_toggle(None, None)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: pause_label.into(),
                activate: Box::new(|_tray| {
                    crate::config::toggle_paused();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_tray| quit()),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Spawn the D-Bus tray service and wire panel copy/quit actions straight into
/// the monitor event loop (zero polling).
#[cfg(target_os = "linux")]
pub fn spawn(
    on_copy: impl Fn(String) + Send + Sync + 'static,
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<Handle<LinuxTray>, ksni::Error> {
    let quit: Arc<dyn Fn() + Send + Sync> = Arc::new(on_quit);
    let tray = LinuxTray {
        quit: Arc::clone(&quit),
    };
    let handle = tray.spawn()?;
    let _ = TRAY_HANDLE.set(handle.clone());

    crate::panel::set_handlers(on_copy, move || quit());

    Ok(handle)
}
