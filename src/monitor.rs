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
//! On native X11 sessions, XFixes selection-owner events wake clipboard
//! processing; Wayland and unknown environments use a slower fallback poll.

#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use crate::clipboard;
use crate::transform::transform_text;

/// Handoff between the clipboard-update handler and the deferred writer
/// (Windows only — on Linux we write directly from the poll callback).
#[cfg(target_os = "windows")]
struct PendingWrite {
    source: String,
    replacement: String,
}

#[cfg(target_os = "windows")]
static PENDING_WRITE: Mutex<Option<PendingWrite>> = Mutex::new(None);

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
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostQuitMessage, RegisterClassExW, TranslateMessage, CS_HREDRAW, CS_VREDRAW, HWND_MESSAGE,
        MSG, WM_CLIPBOARDUPDATE, WM_DESTROY, WNDCLASSEXW,
    };

    /// Custom message: perform the deferred clipboard write.
    /// (WM_APP is 0x8000; we use 0x8002 to leave room for the tray callback at 0x8001.)
    const WM_DO_WRITE: u32 = 0x8002;

    /// `HWND` is `*mut c_void`, which isn't `Send`. Store it as `usize`
    /// (trivially `Send`) and cast back when posting — we never dereference it.
    struct SendHwnd(usize);
    impl SendHwnd {
        fn new(hwnd: HWND) -> Self {
            Self(hwnd as usize)
        }
        fn post_message(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    self.0 as HWND,
                    msg,
                    wparam,
                    lparam,
                ) != 0
            }
        }
    }

    /// Entry point: set up the window + listener, run the message loop, clean up.

    pub fn run() {
        unsafe {
            let class_name = clipboard::wide("AutoFxEmbedListener");

            let mut wc: WNDCLASSEXW = std::mem::zeroed();
            wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
            wc.style = CS_HREDRAW | CS_VREDRAW;
            wc.lpfnWndProc = Some(window_proc);
            wc.lpszClassName = class_name.as_ptr();
            wc.hInstance = GetModuleHandleW(std::ptr::null());

            if RegisterClassExW(&wc) == 0 {
                eprintln!("AutoFxEmbed: failed to register clipboard listener window");
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
                wc.hInstance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                eprintln!("AutoFxEmbed: failed to create clipboard listener window");
                return;
            }

            // Event-driven clipboard monitoring: no polling thread, zero idle CPU.
            if AddClipboardFormatListener(hwnd) == 0 {
                DestroyWindow(hwnd);
                return;
            }

            if !crate::tray::add(hwnd) {
                eprintln!("AutoFxEmbed: failed to create tray icon");
                RemoveClipboardFormatListener(hwnd);
                DestroyWindow(hwnd);
                return;
            }

            let mut msg: MSG = std::mem::zeroed();
            let message_result = loop {
                let result = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                if result <= 0 {
                    break result;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            };
            if message_result < 0 {
                eprintln!("AutoFxEmbed: Windows message loop failed");
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
        unsafe {
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
    }

    /// Runs inside WM_CLIPBOARDUPDATE. Reads the clipboard, transforms, and if it
    /// changed, stashes the new text and POSTS a message to write it later (so we
    /// don't re-enter the clipboard during the update broadcast).
    unsafe fn handle_clipboard_update_win(hwnd: HWND) {
        unsafe {
            let text = (0..3).find_map(|attempt| {
                let text = clipboard::read_text();
                if text.is_none() && attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                text
            });
            let Some(text) = text else {
                eprintln!("AutoFxEmbed: unable to read clipboard text");
                return;
            };
            let Some(new_text) = transform_text(&text) else {
                return;
            };
            if let Ok(mut guard) = PENDING_WRITE.lock() {
                *guard = Some(PendingWrite {
                    source: text,
                    replacement: new_text,
                });
                if windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    WM_DO_WRITE,
                    0,
                    0,
                ) == 0
                {
                    *guard = None;
                    eprintln!("AutoFxEmbed: failed to schedule clipboard write");
                }
            } else {
                eprintln!("AutoFxEmbed: clipboard write queue is unavailable");
            }
        }
    }

    /// Retry a failed write later without blocking the Windows message loop.
    fn retry_pending_write(hwnd: HWND, pending: PendingWrite) {
        let hwnd = SendHwnd::new(hwnd);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let Ok(mut guard) = PENDING_WRITE.lock() else {
                eprintln!("AutoFxEmbed: clipboard write queue is unavailable");
                return;
            };
            // Preserve a newer clipboard update if one arrived while we waited.
            if guard.is_some() {
                return;
            }
            *guard = Some(pending);
            if !hwnd.post_message(WM_DO_WRITE, 0, 0) {
                *guard = None;
                eprintln!("AutoFxEmbed: failed to reschedule clipboard write");
            }
        });
    }

    /// Runs in the deferred WM_DO_WRITE handler, outside the update broadcast.
    fn do_pending_write_win(hwnd: HWND) {
        let pending = PENDING_WRITE.lock().ok().and_then(|mut g| g.take());
        if let Some(pending) = pending {
            for _ in 0..3 {
                match clipboard::read_text() {
                    Some(current) if current == pending.source => {}
                    Some(_) => return,
                    None => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                }
                if clipboard::write_text(&pending.replacement) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            eprintln!("AutoFxEmbed: clipboard write busy; retrying later");
            retry_pending_write(hwnd, pending);
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
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, RecvTimeoutError, Sender},
            Arc,
        },
        thread,
        time::{Duration, Instant},
    };
    use x11rb::{
        connection::Connection,
        protocol::{
            xfixes::{ConnectionExt as XfixesConnectionExt, SelectionEvent, SelectionEventMask},
            xproto::{self, ConnectionExt as XprotoConnectionExt},
            Event,
        },
        rust_connection::RustConnection,
    };

    /// Poll interval for Wayland and other environments without a watcher.
    /// Native X11 sessions use XFixes events, with a slow safety read because
    /// XFixes does not report content changes made by the same owner.
    const FALLBACK_POLL: Duration = Duration::from_millis(30);
    const X11_SAFETY_POLL: Duration = Duration::from_secs(5);
    const QUIT_CHECK: Duration = Duration::from_millis(100);
    const MAX_PRIMARY_BYTES: usize = 1024 * 1024;

    struct X11Watcher {
        changes: Receiver<()>,
        shutdown_connection: RustConnection,
        shutdown_atom: xproto::Atom,
        shutdown_window: xproto::Window,
        thread: thread::JoinHandle<()>,
    }

    impl X11Watcher {
        fn shutdown(self) {
            // Claim a private selection to wake the blocking watcher with a
            // guaranteed owner-change event, even if it was previously empty.
            let X11Watcher {
                changes: _,
                shutdown_connection,
                shutdown_atom,
                shutdown_window,
                thread,
            } = self;
            let _ = shutdown_connection.set_selection_owner(
                shutdown_window,
                shutdown_atom,
                x11rb::CURRENT_TIME,
            );
            let _ = shutdown_connection.flush();
            let _ = thread.join();
            let _ = shutdown_connection.destroy_window(shutdown_window);
            let _ = shutdown_connection.flush();
        }

        fn join(self) {
            let X11Watcher {
                changes: _,
                shutdown_connection,
                shutdown_atom: _,
                shutdown_window,
                thread,
            } = self;
            let _ = thread.join();
            let _ = shutdown_connection.destroy_window(shutdown_window);
            let _ = shutdown_connection.flush();
        }
    }

    /// Start an XFixes watcher when the native desktop session is X11.
    ///
    /// Wayland deliberately falls through: ordinary Wayland clients cannot
    /// passively observe global clipboard ownership without compositor-specific
    /// protocols, while arboard remains the portable read/write implementation.
    fn start_x11_watcher() -> Option<X11Watcher> {
        if std::env::var_os("DISPLAY").is_none() || std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return None;
        }

        let (connection, screen_num) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen_num)?.root;
        let (shutdown_connection, shutdown_screen_num) = x11rb::connect(None).ok()?;
        let shutdown_screen = shutdown_connection.setup().roots.get(shutdown_screen_num)?;
        let shutdown_window = shutdown_connection.generate_id().ok()?;
        shutdown_connection
            .create_window(
                0,
                shutdown_window,
                shutdown_screen.root,
                0,
                0,
                1,
                1,
                0,
                xproto::WindowClass::INPUT_ONLY,
                0,
                &xproto::CreateWindowAux::new(),
            )
            .ok()?
            .check()
            .ok()?;
        shutdown_connection.flush().ok()?;
        connection.xfixes_query_version(5, 0).ok()?.reply().ok()?;
        let clipboard_atom = connection
            .intern_atom(false, b"CLIPBOARD")
            .ok()?
            .reply()
            .ok()?
            .atom;
        let shutdown_atom = connection
            .intern_atom(false, b"AUTOFXEMBED_SHUTDOWN")
            .ok()?
            .reply()
            .ok()?
            .atom;
        for selection in [clipboard_atom, shutdown_atom] {
            connection
                .xfixes_select_selection_input(
                    root,
                    selection,
                    SelectionEventMask::SET_SELECTION_OWNER,
                )
                .ok()?
                .check()
                .ok()?;
        }
        connection.flush().ok()?;

        let (sender, receiver) = mpsc::channel();
        let watcher_thread = thread::spawn(move || {
            while let Ok(event) = connection.wait_for_event() {
                if let Event::XfixesSelectionNotify(event) = event {
                    if event.selection == shutdown_atom {
                        break;
                    }
                    if event.selection == clipboard_atom
                        && event.subtype == SelectionEvent::SET_SELECTION_OWNER
                        && sender.send(()).is_err()
                    {
                        break;
                    }
                }
            }
        });
        Some(X11Watcher {
            changes: receiver,
            shutdown_connection,
            shutdown_atom,
            shutdown_window,
            thread: watcher_thread,
        })
    }

    struct PrimaryReader {
        results: Receiver<Option<String>>,
        sender: Sender<Option<String>>,
        active: Arc<AtomicBool>,
    }

    impl PrimaryReader {
        fn new() -> Self {
            let (sender, results) = mpsc::channel();
            Self {
                results,
                sender,
                active: Arc::new(AtomicBool::new(false)),
            }
        }

        fn request(&self) {
            if self
                .active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                return;
            }
            let sender = self.sender.clone();
            let active = Arc::clone(&self.active);
            thread::spawn(move || {
                let result = read_primary_text();
                let _ = sender.send(result);
                active.store(false, Ordering::Release);
            });
        }

        fn take(&self) -> Option<String> {
            self.results.try_recv().ok().flatten()
        }
    }

    /// Read PRIMARY away from the event loop: a broken owner can block its worker,
    /// but cannot freeze the tray or spawn more than one blocked read.
    fn read_primary_text() -> Option<String> {
        use std::io::Read;
        use wl_clipboard_rs::paste::{get_contents, ClipboardType, MimeType, Seat};
        let pipe = get_contents(ClipboardType::Primary, Seat::Unspecified, MimeType::Text)
            .ok()?
            .0;
        let mut buf = Vec::new();
        pipe.take((MAX_PRIMARY_BYTES + 1) as u64)
            .read_to_end(&mut buf)
            .ok()?;
        if buf.len() > MAX_PRIMARY_BYTES {
            eprintln!("AutoFxEmbed: primary selection is too large");
            return None;
        }
        Some(String::from_utf8_lossy(&buf).into_owned())
    }

    /// Write the PRIMARY selection (best effort — Wayland only).
    fn set_primary(text: &str) -> bool {
        use wl_clipboard_rs::copy::{ClipboardType, MimeType, Options, Source};
        let source = Source::Bytes(text.as_bytes().to_vec().into_boxed_slice());
        let mut opts = Options::new();
        opts.clipboard(ClipboardType::Primary);
        match opts.copy(source, MimeType::Text) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("AutoFxEmbed: primary write failed: {error}");
                false
            }
        }
    }

    fn preview(text: &str) -> String {
        text.chars().take(80).collect()
    }

    pub fn run() {
        let (tray_handle, quit_flag) = match crate::tray::spawn() {
            Ok((handle, quit)) => (handle, quit),
            Err(error) => {
                eprintln!("AutoFxEmbed: tray unavailable: {error:?}");
                return;
            }
        };

        // On Wayland clipboard data is hosted by the application, so we must
        // keep a single persistent Clipboard instance alive — otherwise any
        // text we write disappears before another app can paste it.
        let mut clipboard = match arboard::Clipboard::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("AutoFxEmbed: clipboard unavailable: {e}");
                tray_handle.shutdown().wait();
                return;
            }
        };

        // Track the last clipboard text we saw so we can detect real changes
        // (and avoid re-writing our own transformed text back into the loop).
        let mut last_text = String::new();
        let mut last_primary = String::new();
        let mut failed_text: Option<String> = None;
        let mut retry_at = Instant::now();
        let mut failed_primary: Option<String> = None;
        let mut primary_retry_at = Instant::now();
        let primary_reader = PrimaryReader::new();
        let mut was_empty = false;
        let mut x11_watcher = start_x11_watcher();
        let mut next_safety_poll = Instant::now();

        loop {
            if quit_flag.load(Ordering::SeqCst) {
                break;
            }

            let (watcher_disconnected, process_clipboard, safety_poll_due) = match x11_watcher
                .as_ref()
            {
                Some(watcher) => {
                    let now = Instant::now();
                    let safety_wait = next_safety_poll.saturating_duration_since(now);
                    let retry_wait = failed_text
                        .as_ref()
                        .map(|_| retry_at.saturating_duration_since(now))
                        .unwrap_or(Duration::MAX);

                    let wait_until = safety_wait.min(retry_wait).min(QUIT_CHECK);
                    match watcher.changes.recv_timeout(wait_until) {
                        Ok(()) => {
                            // Coalesce a burst of owner changes into one read.
                            watcher.changes.try_iter().for_each(drop);
                            (false, true, false)
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            let safety_due = safety_wait <= wait_until;
                            let retry_due = retry_wait <= wait_until;
                            (false, safety_due || retry_due, safety_due)
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            eprintln!("AutoFxEmbed: X11 clipboard watcher stopped; using fallback polling");
                            (true, true, false)
                        }
                    }
                }
                None => {
                    thread::sleep(FALLBACK_POLL);
                    (false, true, false)
                }
            };
            if watcher_disconnected {
                if let Some(watcher) = x11_watcher.take() {
                    watcher.join();
                }
            }
            if safety_poll_due {
                next_safety_poll = Instant::now() + X11_SAFETY_POLL;
            }

            // ---- clipboard processing ----
            if process_clipboard {
                // CLIPBOARD selection (Ctrl+C, copy buttons).
                match clipboard.get_text() {
                    Ok(text) => {
                        if text != last_text {
                            was_empty = false;
                            eprintln!("AutoFxEmbed: read -> {:?}", preview(&text));
                        }
                        if text != last_text
                            && (failed_text.as_deref() != Some(text.as_str())
                                || Instant::now() >= retry_at)
                        {
                            if let Some(new_text) = transform_text(&text) {
                                eprintln!(
                                    "AutoFxEmbed: {} -> {}",
                                    preview(&text),
                                    preview(&new_text)
                                );
                                match clipboard.set_text(&new_text) {
                                    Ok(()) => {
                                        if set_primary(&new_text) {
                                            last_primary = new_text.clone();
                                            failed_primary = None;
                                        }
                                        last_text = new_text;
                                        failed_text = None;
                                    }
                                    Err(error) => {
                                        failed_text = Some(text.clone());
                                        retry_at = Instant::now() + Duration::from_secs(1);
                                        eprintln!("AutoFxEmbed: clipboard write failed: {error}");
                                    }
                                }
                            } else {
                                last_text = text;
                                failed_text = None;
                            }
                        }
                    }
                    Err(_) => {
                        if !was_empty {
                            eprintln!("AutoFxEmbed: read error: clipboard empty/unavailable");
                            was_empty = true;
                        }
                    }
                }
            }

            // PRIMARY selection (mouse selection / middle-click paste) — some apps
            // publish copies only there. Reads happen on a bounded worker so a
            // broken selection owner cannot block this event loop.
            if let Some(text) = primary_reader.take() {
                let can_retry = failed_primary.as_deref() != Some(text.as_str())
                    || Instant::now() >= primary_retry_at;
                if text != last_primary && text != last_text && can_retry {
                    if let Some(new_text) = transform_text(&text) {
                        eprintln!(
                            "AutoFxEmbed: primary {} -> {}",
                            preview(&text),
                            preview(&new_text)
                        );
                        match clipboard.set_text(&new_text) {
                            Ok(()) => last_text = new_text.clone(),
                            Err(error) => {
                                eprintln!("AutoFxEmbed: clipboard write failed: {error}");
                            }
                        }
                        if set_primary(&new_text) {
                            last_primary = new_text;
                            failed_primary = None;
                        } else {
                            failed_primary = Some(text);
                            primary_retry_at = Instant::now() + Duration::from_secs(1);
                        }
                    } else {
                        last_primary = text;
                        failed_primary = None;
                    }
                }
            }

            let primary_retry_due = failed_primary.is_none() || Instant::now() >= primary_retry_at;
            if (process_clipboard || failed_primary.is_some()) && primary_retry_due {
                primary_reader.request();
            }
        }

        if let Some(watcher) = x11_watcher {
            watcher.shutdown();
        }

        // Graceful shutdown: unregister from D-Bus and wait.
        tray_handle.shutdown().wait();
    }
}

// =========================================================================
// Public entry point
// =========================================================================

/// Keep a single instance: two instances racing on clipboard rewrites produce
/// "sometimes transforms, sometimes not" behavior.
/// The OS file lock auto-releases when the process dies, so no stale locks.
fn lock_single_instance() -> Option<std::fs::File> {
    let dir =
        dirs::runtime_dir().or_else(|| dirs::config_dir().map(|path| path.join("autofxembed")))?;
    std::fs::create_dir_all(&dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(dir.join("autofxembed.lock"))
        .ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(_) => None,
    }
}
/// Start the clipboard monitor + tray icon.  Blocks until the user quits.
pub fn run() {
    let Some(_single_instance) = lock_single_instance() else {
        eprintln!("AutoFxEmbed: another instance is already running; exiting");
        return;
    };
    crate::config::load();
    #[cfg(target_os = "windows")]
    win::run();

    #[cfg(target_os = "linux")]
    linux_impl::run();

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        eprintln!("AutoFxEmbed: unsupported platform");
    }
}
