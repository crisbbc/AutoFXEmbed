use arboard::Clipboard;

/// Read the current clipboard text. Returns `None` if the clipboard has no
/// text or cannot be opened (e.g. no display server on Linux).
pub fn read_text() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    clipboard.get_text().ok()
}

/// Write `text` to the clipboard, replacing current contents. Returns `true`
/// on success.
pub fn write_text(text: &str) -> bool {
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(_) => return false,
    };
    clipboard.set_text(text).is_ok()
}

/// Encode a Rust &str as a null-terminated UTF-16 buffer.
/// Only used by the Windows tray and autostart modules.
#[cfg(target_os = "windows")]
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
