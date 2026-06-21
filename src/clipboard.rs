use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

/// Encode a Rust &str as a null-terminated UTF-16 buffer (used by tray strings too).
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Read the current clipboard text. Returns `None` if the clipboard has no
/// text or cannot be opened (e.g. locked by another app).
///
/// # Safety
/// Calls Win32 clipboard APIs.
pub unsafe fn read_text(hwnd: HWND) -> Option<String> {
    if OpenClipboard(hwnd) == 0 {
        return None;
    }
    let result = read_text_inner();
    CloseClipboard();
    result
}

unsafe fn read_text_inner() -> Option<String> {
    let h = GetClipboardData(CF_UNICODETEXT as u32);
    if h.is_null() {
        return None;
    }
    let ptr = GlobalLock(h) as *const u16;
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    let s = String::from_utf16_lossy(slice);
    GlobalUnlock(h);
    Some(s)
}

/// Write `text` to the clipboard, replacing current contents. Returns `true`
/// on success.
///
/// # Safety
/// Calls Win32 clipboard + global memory APIs. After a successful
/// `SetClipboardData` the system owns the memory; we must NOT free it.
pub unsafe fn write_text(hwnd: HWND, text: &str) -> bool {
    let buf: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = buf.len() * 2;

    if OpenClipboard(hwnd) == 0 {
        return false;
    }
    EmptyClipboard();
    let ok = write_text_inner(&buf, byte_len);
    CloseClipboard();
    ok
}

unsafe fn write_text_inner(buf: &[u16], byte_len: usize) -> bool {
    let h = GlobalAlloc(GMEM_MOVEABLE, byte_len);
    if h.is_null() {
        return false;
    }
    let ptr = GlobalLock(h) as *mut u16;
    if ptr.is_null() {
        return false;
    }
    std::ptr::copy_nonoverlapping(buf.as_ptr(), ptr, buf.len());
    GlobalUnlock(h);
    // On success the system owns `h`; do NOT GlobalFree it. On failure the
    // tiny leak is acceptable (rare path).
    !SetClipboardData(CF_UNICODETEXT as u32, h).is_null()
}
