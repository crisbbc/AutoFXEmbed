//! Windows "Run" registry key support for start-on-startup.
//!
//! We add/remove a value under
//! `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` so Windows
//! launches AutoFxEmbed at logon. The registry IS the persisted state — the
//! tray menu reads it fresh on each right-click.
//!
//! ## Linux
//!
//! On Linux we create/remove a `.desktop` file under
//! `$XDG_CONFIG_HOME/autostart/` (or `~/.config/autostart/`).

#[cfg(target_os = "windows")]
use crate::clipboard::wide; // reuse the existing null-terminated UTF-16 helper (DRY)

// ---------------------------------------------------------------------------
// cross-platform public API
// ---------------------------------------------------------------------------

/// `true` if the auto-start Run value currently exists.
pub fn is_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        unsafe { is_enabled_windows() }
    }
    #[cfg(target_os = "linux")]
    {
        is_enabled_linux()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

/// Toggle auto-start on/off and return the new enabled state.
pub fn toggle() -> bool {
    #[cfg(target_os = "windows")]
    {
        unsafe { toggle_windows() }
    }
    #[cfg(target_os = "linux")]
    {
        toggle_linux()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Windows implementation (registry)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

/// Registry subkey under HKEY_CURRENT_USER that Windows reads at logon.
#[cfg(target_os = "windows")]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// The value name we use inside that key.
#[cfg(target_os = "windows")]
const VALUE_NAME: &str = "AutoFxEmbed";

/// `true` if the auto-start Run value currently exists (and is a string).
///
/// # Safety
/// Calls Win32 registry APIs.
#[cfg(target_os = "windows")]
unsafe fn is_enabled_windows() -> bool {
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
#[cfg(target_os = "windows")]
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
#[cfg(target_os = "windows")]
unsafe fn enable_windows() -> bool {
    let sub = wide(RUN_KEY);
    let val = wide(VALUE_NAME);
    let mut hkey: HKEY = std::ptr::null_mut();
    if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) != ERROR_SUCCESS
    {
        return false;
    }
    let data = quoted_exe_path();
    let cb = (data.len() * 2) as u32; // bytes, including the trailing null
    let rc = RegSetValueExW(
        hkey,
        val.as_ptr(),
        0,
        REG_SZ,
        data.as_ptr() as *const u8,
        cb,
    );
    RegCloseKey(hkey);
    rc == ERROR_SUCCESS
}

/// Disable auto-start by deleting the Run value. Returns `true` on success
/// (also `true` if the value was already absent — desired state reached).
///
/// # Safety
/// Calls Win32 registry APIs.
#[cfg(target_os = "windows")]
unsafe fn disable_windows() -> bool {
    let sub = wide(RUN_KEY);
    let val = wide(VALUE_NAME);
    let mut hkey: HKEY = std::ptr::null_mut();
    if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) != ERROR_SUCCESS
    {
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
#[cfg(target_os = "windows")]
unsafe fn toggle_windows() -> bool {
    if is_enabled_windows() {
        let _ = disable_windows();
    } else {
        let _ = enable_windows();
    }
    is_enabled_windows()
}

// ---------------------------------------------------------------------------
// Linux implementation (.desktop file in XDG autostart)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn autostart_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|p| p.join("autostart"))
}

#[cfg(target_os = "linux")]
fn desktop_path() -> Option<std::path::PathBuf> {
    autostart_dir().map(|p| p.join("autofxembed.desktop"))
}

#[cfg(target_os = "linux")]
fn is_enabled_linux() -> bool {
    desktop_path().is_some_and(|p| p.exists())
}

#[cfg(target_os = "linux")]
fn exe_path() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| String::new())
}

#[cfg(target_os = "linux")]
fn desktop_exec_arg(path: &str) -> String {
    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for character in path.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(target_os = "linux")]
fn desktop_file_content() -> String {
    format!(
        "\
[Desktop Entry]
Type=Application
Name=AutoFxEmbed
Comment=Auto-rewrite social-media links in the clipboard to FxEmbed
Exec={exe}
Terminal=false
X-GNOME-Autostart-enabled=true
",
        exe = desktop_exec_arg(&exe_path())
    )
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::desktop_exec_arg;

    #[test]
    fn quotes_and_escapes_desktop_exec_paths() {
        assert_eq!(
            desktop_exec_arg("/home/user/My Apps/auto\\fx\"embed"),
            "\"/home/user/My Apps/auto\\\\fx\\\"embed\""
        );
    }
}

#[cfg(target_os = "linux")]
fn enable_linux() -> bool {
    match desktop_path() {
        Some(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::write(&path, desktop_file_content()).is_ok()
        }
        None => false,
    }
}

#[cfg(target_os = "linux")]
fn disable_linux() -> bool {
    match desktop_path() {
        Some(path) => {
            if path.exists() {
                std::fs::remove_file(&path).is_ok()
            } else {
                true // already absent
            }
        }
        None => false,
    }
}

#[cfg(target_os = "linux")]
fn toggle_linux() -> bool {
    if is_enabled_linux() {
        disable_linux();
        false
    } else {
        enable_linux()
    }
}
