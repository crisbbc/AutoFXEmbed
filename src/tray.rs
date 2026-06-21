use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IMAGE_ICON, LoadImageW,
    LR_DEFAULTSIZE, LR_SHARED, MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_DEFAULT, MF_SEPARATOR,
    MF_STRING, MessageBoxW, TrackPopupMenu, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_TOPALIGN, WM_RBUTTONUP,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

/// Custom message Windows sends to our window when the tray icon is interacted with.
/// (WM_APP = 0x8000.)
pub const TRAY_CALLBACK_MSG: u32 = 0x8001;

/// Menu item IDs.
const MENU_QUIT: u32 = 1;
const MENU_STARTUP: u32 = 2;
const MENU_ABOUT: u32 = 3;

/// # Safety
/// Calls Win32 shell + user32 APIs.
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
