//! Windows "Run" registry key support for start-on-startup.
//!
//! We add/remove a value under
//! `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` so Windows
//! launches AutoFxEmbed at logon. The registry IS the persisted state — the
//! tray menu reads it fresh on each right-click.

use crate::clipboard::wide; // reuse the existing null-terminated UTF-16 helper (DRY)
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

/// Registry subkey under HKEY_CURRENT_USER that Windows reads at logon.
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// The value name we use inside that key.
const VALUE_NAME: &str = "AutoFxEmbed";

/// `true` if the auto-start Run value currently exists (and is a string).
///
/// # Safety
/// Calls Win32 registry APIs.
pub unsafe fn is_enabled() -> bool {
    let sub = wide(RUN_KEY);
    let val = wide(VALUE_NAME);
    let mut hkey: HKEY = std::ptr::null_mut();
    if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
        return false;
    }
    let mut ty: u32 = 0;
    let mut cb: u32 = 0;
    let rc = RegQueryValueExW(
        hkey,
        val.as_ptr(),
        std::ptr::null(),
        &mut ty,
        std::ptr::null_mut(),
        &mut cb,
    );
    RegCloseKey(hkey);
    rc == ERROR_SUCCESS && ty == REG_SZ
}

/// Return the current exe path wrapped in double quotes, null-terminated UTF-16.
/// The quoting lets paths with spaces survive the Run key's command-line parsing.
///
/// # Safety
/// Calls GetModuleFileNameW.
unsafe fn quoted_exe_path() -> Vec<u16> {
    let mut buf = [0u16; 1024];
    let len = GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
    if len == 0 {
        return wide("");
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    wide(&format!("\"{}\"", path))
}

/// Enable auto-start by writing the Run value. Returns `true` on success.
///
/// # Safety
/// Calls Win32 registry APIs.
pub unsafe fn enable() -> bool {
    let sub = wide(RUN_KEY);
    let val = wide(VALUE_NAME);
    let mut hkey: HKEY = std::ptr::null_mut();
    if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) != ERROR_SUCCESS {
        return false;
    }
    let data = quoted_exe_path();
    let cb = (data.len() * 2) as u32; // bytes, including the trailing null
    let rc = RegSetValueExW(hkey, val.as_ptr(), 0, REG_SZ, data.as_ptr() as *const u8, cb);
    RegCloseKey(hkey);
    rc == ERROR_SUCCESS
}

/// Disable auto-start by deleting the Run value. Returns `true` on success
/// (also `true` if the value was already absent — desired state reached).
///
/// # Safety
/// Calls Win32 registry APIs.
pub unsafe fn disable() -> bool {
    let sub = wide(RUN_KEY);
    let val = wide(VALUE_NAME);
    let mut hkey: HKEY = std::ptr::null_mut();
    if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) != ERROR_SUCCESS {
        return false;
    }
    let rc = RegDeleteValueW(hkey, val.as_ptr());
    RegCloseKey(hkey);
    rc == ERROR_SUCCESS || rc == ERROR_FILE_NOT_FOUND
}

/// Toggle auto-start on/off and return the new enabled state.
///
/// # Safety
/// Calls Win32 registry APIs.
pub unsafe fn toggle() -> bool {
    if is_enabled() {
        disable();
        false
    } else if enable() {
        true
    } else {
        is_enabled()
    }
}
