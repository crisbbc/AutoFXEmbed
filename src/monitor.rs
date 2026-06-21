use std::sync::Mutex;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, DestroyWindow, GetMessageW,
    PostMessageW, PostQuitMessage, RegisterClassExW, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE, WM_DESTROY, WNDCLASSEXW,
};

use crate::clipboard;
use crate::transform::transform_text;

/// Custom message: perform the deferred clipboard write.
/// (WM_APP is 0x8000; we use 0x8002 to leave room for the tray callback at 0x8001.)
const WM_DO_WRITE: u32 = 0x8002;

/// Handoff between the clipboard-update handler and the deferred writer.
static PENDING_WRITE: Mutex<Option<String>> = Mutex::new(None);

/// Entry point: set up the window + listener, run the message loop, clean up.
pub fn run() {
    unsafe {
        let class_name = clipboard::wide("AutoFxEmbedListener");

        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.lpfnWndProc = Some(window_proc);
        wc.lpszClassName = class_name.as_ptr();

        if RegisterClassExW(&wc) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            clipboard::wide("AutoFxEmbed").as_ptr(),
            0,
            0, 0, 0, 0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }

        // Event-driven clipboard monitoring: no polling thread, zero idle CPU.
        if AddClipboardFormatListener(hwnd) == 0 {
            DestroyWindow(hwnd);
            return;
        }

        crate::tray::add(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        crate::tray::remove(hwnd);
        RemoveClipboardFormatListener(hwnd);
        DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => {
            handle_clipboard_update(hwnd);
            0
        }
        WM_DO_WRITE => {
            do_pending_write(hwnd);
            0
        }
        crate::tray::TRAY_CALLBACK_MSG => {
            crate::tray::handle_event(hwnd, lparam);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Runs inside WM_CLIPBOARDUPDATE. Reads the clipboard, transforms, and if it
/// changed, stashes the new text and POSTS a message to write it later (so we
/// don't re-enter the clipboard during the update broadcast).
unsafe fn handle_clipboard_update(hwnd: HWND) {
    let Some(text) = clipboard::read_text(hwnd) else {
        return;
    };
    let Some(new_text) = transform_text(&text) else {
        return; // not a link we handle (or already transformed) -> no write, no loop
    };
    if let Ok(mut guard) = PENDING_WRITE.lock() {
        *guard = Some(new_text);
        PostMessageW(hwnd, WM_DO_WRITE, 0, 0);
    }
}

/// Runs in the deferred WM_DO_WRITE handler, outside the update broadcast.
unsafe fn do_pending_write(hwnd: HWND) {
    let to_write = PENDING_WRITE.lock().ok().and_then(|mut g| g.take());
    if let Some(new_text) = to_write {
        clipboard::write_text(hwnd, &new_text);
    }
}
