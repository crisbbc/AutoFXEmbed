//! Clipboard-monitor / event-loop entry points.
//!
//! ## Windows
//!
//! Creates a message-only window, registers as a clipboard-format listener,
//! adds the tray icon, and runs the standard Win32 message pump.
//! Clipboard updates arrive via `WM_CLIPBOARDUPDATE`; tray events via
//! `TRAY_CALLBACK_MSG`.
//!
//! ## Linux
//!
//! Uses `ksni` (pure-Rust D-Bus StatusNotifierItem) for the tray icon —
//! zero X11/GTK dependencies, works on both X11 and Wayland.
//! Clipboard is polled on a simple sleep loop; tray menu events are handled
//! via the `activate` callbacks on each menu item.

#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use crate::clipboard;
use crate::transform::transform_text;

/// Handoff between the clipboard-update handler and the deferred writer
/// (Windows only — on Linux we write directly from the poll callback).
#[cfg(target_os = "windows")]
static PENDING_WRITE: Mutex<Option<String>> = Mutex::new(None);

// =========================================================================
// Windows
// =========================================================================

#[cfg(target_os = "windows")]
mod win {
    use super::*;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::DataExchange::{
        AddClipboardFormatListener, RemoveClipboardFormatListener,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, DestroyWindow, GetMessageW,
        PostMessageW, PostQuitMessage, RegisterClassExW, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
        HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE, WM_DESTROY, WNDCLASSEXW,
    };

    /// Custom message: perform the deferred clipboard write.
    /// (WM_APP is 0x8000; we use 0x8002 to leave room for the tray callback at 0x8001.)
    const WM_DO_WRITE: u32 = 0x8002;

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
                0,
                0,
                0,
                0,
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
                handle_clipboard_update_win(hwnd);
                0
            }
            WM_DO_WRITE => {
                do_pending_write_win(hwnd);
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
    unsafe fn handle_clipboard_update_win(hwnd: HWND) {
        let Some(text) = clipboard::read_text() else {
            return;
        };
        let Some(new_text) = transform_text(&text) else {
            return;
        };
        if let Ok(mut guard) = PENDING_WRITE.lock() {
            *guard = Some(new_text);
            PostMessageW(hwnd, WM_DO_WRITE, 0, 0);
        }
    }

    /// Runs in the deferred WM_DO_WRITE handler, outside the update broadcast.
    unsafe fn do_pending_write_win(_hwnd: HWND) {
        let to_write = PENDING_WRITE.lock().ok().and_then(|mut g| g.take());
        if let Some(new_text) = to_write {
            clipboard::write_text(&new_text);
        }
    }
}

// =========================================================================
// Linux
// =========================================================================

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use std::{
        sync::atomic::Ordering,
        time::Duration,
    };

    /// Poll interval for clipboard checks (milliseconds).
    /// 100 ms gives near-instant response while keeping CPU usage negligible.
    const POLL_MS: u64 = 100;

    pub fn run() {
        let (_handle, quit_flag) = crate::tray::spawn();

        // On Wayland clipboard data is hosted by the application, so we must
        // keep a single persistent Clipboard instance alive — otherwise any
        // text we write disappears before another app can paste it.
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("AutoFxEmbed: clipboard unavailable: {e}");
                return;
            }
        };

        // Track the last clipboard text we saw so we can detect real changes
        // (and avoid re-writing our own transformed text back into the loop).
        let mut last_text = String::new();

        loop {
            if quit_flag.load(Ordering::SeqCst) {
                break;
            }

            // ---- clipboard polling ----
            if let Ok(text) = clipboard.get_text() {
                if text != last_text {
                    last_text = text.clone();
                    if let Some(new_text) = transform_text(&text) {
                        last_text = new_text.clone();
                        if let Err(e) = clipboard.set_text(&new_text) {
                            eprintln!("AutoFxEmbed: clipboard write failed: {e}");
                        }
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(POLL_MS));
        }

        // Graceful shutdown: unregister from D-Bus and wait.
        _handle.shutdown().wait();
    }
}

// =========================================================================
// Public entry point
// =========================================================================

/// Start the clipboard monitor + tray icon.  Blocks until the user quits.
pub fn run() {
    #[cfg(target_os = "windows")]
    win::run();

    #[cfg(target_os = "linux")]
    linux_impl::run();

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        eprintln!("AutoFxEmbed: unsupported platform");
    }
}
