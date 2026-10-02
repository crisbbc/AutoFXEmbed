//! System tray icon.
//!
//! Left-clicking the icon opens the flyout panel (see [`crate::panel`]);
//! right-clicking shows a tiny Open / Quit menu as a fallback.
//!
//! ## Windows
//!
//! Uses raw Win32 shell + user32 APIs to create a tray icon and popup menu.
//! Tray events arrive via the window message pump in [`crate::monitor`].
//!
//! ## Linux
//!
//! Uses `ksni` to expose a pure-Rust D-Bus StatusNotifierItem. Activation
//! (left-click) opens the panel; the menu offers Open and Quit.
//!
//! The panel is a separate process, so its copy / quit actions reach the
//! monitor loop through handlers registered with [`crate::panel::set_handlers`].

/// Text of the About message: the running version plus the credit line.
pub(crate) fn about_text() -> String {
    format!(
        "AutoFxEmbed v{}\nMade by Cris with much <3 for his friends",
        env!("CARGO_PKG_VERSION")
    )
}

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
const MENU_QUIT: u32 = 2;


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
/// Left-click toggles the panel; right-click shows the Open / Quit menu.
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
    if mouse_msg == WM_LBUTTONUP {
        crate::panel::open_or_toggle(anchor);
        return;
    }

    let Some(menu) = Menu::popup() else {
        return;
    };
    if !menu.append(MF_STRING | MF_DEFAULT, MENU_OPEN as usize, "Open")
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
        MENU_OPEN => crate::panel::open_or_toggle(anchor),
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
    menu::{MenuItem, StandardItem},
    Icon, Tray as KsniTray,
};

/// Tray data that implements the ksni [`KsniTray`] trait.
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

    /// Left-click: ksni passes the click position in screen coordinates.
    fn activate(&mut self, x: i32, y: i32) {
        crate::panel::open_or_toggle(Some((x as f32, y as f32)));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let quit = self.quit_requested.clone();
        vec![
            StandardItem {
                label: "Open".into(),
                activate: Box::new(|_tray| crate::panel::open_or_toggle(None)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_tray| quit.store(true, Ordering::SeqCst)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// What `spawn` hands back to the monitor loop.
#[cfg(target_os = "linux")]
pub struct TrayControl {
    pub handle: Handle<LinuxTray>,
    pub quit: Arc<AtomicBool>,
    /// Embed link the user picked from the panel's Recent list, waiting to be
    /// copied. The monitor loop owns the clipboard (on Wayland it must stay
    /// alive to serve pastes) and copies it.
    pub copy_request: Arc<Mutex<Option<String>>>,
}

/// Spawn the D-Bus tray service.
#[cfg(target_os = "linux")]
pub fn spawn() -> Result<TrayControl, ksni::Error> {
    let quit = Arc::new(AtomicBool::new(false));
    let copy_request = Arc::new(Mutex::new(None));
    let tray = LinuxTray {
        quit_requested: quit.clone(),
    };
    let handle = tray.spawn()?;

    // The panel is another process: route its actions back to this one.
    let copy_slot = copy_request.clone();
    let quit_flag = quit.clone();
    crate::panel::set_handlers(
        move |embed| *copy_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(embed),
        move || quit_flag.store(true, Ordering::SeqCst),
    );

    Ok(TrayControl {
        handle,
        quit,
        copy_request,
    })
}
