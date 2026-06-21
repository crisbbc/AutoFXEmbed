# AutoFxEmbed Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** A lightweight, resource-friendly Windows background app (Rust) that watches the clipboard and, when it contains a single X / Twitter / Bluesky link, rewrites it in place to its FxEmbed form so Discord/Telegram embed it properly.

**Architecture:** A single-threaded Win32 message loop driving a *message-only* (invisible) window. The window is registered with `AddClipboardFormatListener`, so it wakes only on real clipboard changes — zero polling, zero CPU at idle. On `WM_CLIPBOARDUPDATE` it reads the clipboard text, runs a pure `transform` function, and if the link matched it posts a deferred write message (to avoid re-entering the clipboard during the broadcast); the deferred handler writes the transformed URL back. The already-transformed form never re-triggers a write, so there is no infinite loop. A minimal system-tray icon provides a right-click "Quit" menu. The pure transform logic lives in its own module and is fully unit-tested; the Win32 glue is verified by manual end-to-end checks.

**Tech Stack:** Rust 2021 edition, `windows-sys` 0.61 (zero-overhead raw Win32 FFI bindings — the lightest Windows binding available), `std::sync::Mutex` for the deferred-write handoff. No polling, no async runtime, no logging framework, no UI framework. Release profile: `opt-level="z"` + LTO + `panic="abort"` + strip → tiny binary.

---

## FxEmbed transformation rules (from the README)

| Original host        | FxEmbed host     | Prefix inserted before the host |
|----------------------|------------------|---------------------------------|
| `twitter.com`        | `fxtwitter.com`  | `fx`                            |
| `x.com`              | `fixupx.com`     | `fixup`                         |
| `bsky.app`           | `fxbsky.app`     | `fx`                            |

The transformation is a host-only rewrite: everything else (scheme, subdomain, path, query, fragment) is preserved. Only clean single-URL clipboard content is rewritten; prose and already-transformed links are left alone.

---

## Project layout (final)

```
autofxembed/
  Cargo.toml
  src/
    lib.rs          — pub mod re-exports
    transform.rs    — pure URL rewrite logic (unit-tested)
    clipboard.rs    — Win32 clipboard read/write + wide-string helper
    monitor.rs      — window class, message-only window, clipboard listener, message loop, window proc
    tray.rs         — system-tray icon + right-click "Quit" popup menu
    main.rs         — #![windows_subsystem = "windows"] + calls monitor::run()
  tests/
    transform.rs    — integration tests for the pure transform logic (TDD)
```

This is a **lib + bin** crate (`src/lib.rs` present), so both `cargo test <filter>` and `cargo test --lib <filter>` are valid, and integration tests under `tests/` work.

---

## Environment setup (do this once, every cargo invocation)

This host's cargo/rustc live in `~/.cargo/bin`, which is **not** on the default git-bash PATH. Prefix every cargo command:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

The MSVC linker (`link.exe`) is already installed and verified working (memory). All build/test commands in this plan run on Windows from the project root `C:\Users\bbccris\Documents\Proyectos\AutoFXEmbed`.

---

## Task 1: Scaffold the Cargo project

**Objective:** Create the crate skeleton that compiles (with empty modules) so every later task has a working baseline.

**Files:**
- Create: `Cargo.toml`
- Create: `src/lib.rs`
- Create: `src/main.rs`
- Create: `src/transform.rs`
- Create: `src/clipboard.rs`
- Create: `src/monitor.rs`
- Create: `src/tray.rs`
- Create: `tests/transform.rs` (empty stub for now)
- Create: `.gitignore`

**Step 1: Create `Cargo.toml`**

```toml
[package]
name = "autofxembed"
version = "0.1.0"
edition = "2021"

[dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_DataExchange",
    "Win32_System_Ole",
    "Win32_System_Memory",
    "Win32_UI_Shell",
] }

[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = "symbols"
panic = "abort"
```

**Step 2: Create `src/lib.rs`**

```rust
pub mod transform;
pub mod clipboard;
pub mod monitor;
pub mod tray;
```

**Step 3: Create `src/transform.rs` (empty stub — TDD fills it in Task 2)**

```rust
// Implemented in Task 2 (TDD).
```

**Step 4: Create `src/clipboard.rs` (empty stub)**

```rust
// Implemented in Task 5.
```

**Step 5: Create `src/monitor.rs` (empty stub)**

```rust
// Implemented in Tasks 6-7.
```

**Step 6: Create `src/tray.rs` (empty stub)**

```rust
// Implemented in Tasks 8-9.
```

**Step 7: Create `src/main.rs`**

```rust
#![windows_subsystem = "windows"]

fn main() {
    autofxembed::monitor::run();
}
```

> Note: `#![windows_subsystem = "windows"]` makes the final `.exe` a GUI-subsystem binary with **no console window** — essential for a background tray utility. While debugging you can temporarily comment it out to see `println!` output; restore it before release.

**Step 8: Create `tests/transform.rs` (empty stub)**

```rust
// Tests added in Task 2 (TDD).
```

**Step 9: Create `.gitignore`**

```text
/target
*.exe
*.pdb
```

**Step 10: Build to verify the skeleton compiles**

Run (from project root, after `export PATH="$HOME/.cargo/bin:$PATH"`):
```bash
cargo build
```
Expected: `Finished` (warnings about unused modules are fine). If `windows-sys` fails to fetch, ensure network access; the version `0.61` resolves to the latest 0.61.x (currently 0.61.2).

**Step 11: Init git and commit**

```bash
git init
git add -A
git commit -m "chore: scaffold autofxembed crate"
```

---

## Task 2: Transform logic — happy path (TDD)

**Objective:** Implement the core URL rewrite for the three hosts with an `https://` scheme, via TDD.

**Files:**
- Modify: `tests/transform.rs`
- Modify: `src/transform.rs`

**Step 1: Write failing tests in `tests/transform.rs`**

```rust
use autofxembed::transform::transform_clipboard;

#[test]
fn transforms_twitter_https() {
    assert_eq!(
        transform_clipboard("https://twitter.com/user/status/123"),
        Some("https://fxtwitter.com/user/status/123".to_string())
    );
}

#[test]
fn transforms_x_https() {
    assert_eq!(
        transform_clipboard("https://x.com/user/status/123"),
        Some("https://fixupx.com/user/status/123".to_string())
    );
}

#[test]
fn transforms_bsky_https() {
    assert_eq!(
        transform_clipboard("https://bsky.app/profile/user.bsky.social"),
        Some("https://fxbsky.app/profile/user.bsky.social".to_string())
    );
}
```

**Step 2: Run tests to verify failure**

Run:
```bash
cargo test --test transform
```
Expected: FAIL — `unresolved import autofxembed::transform::transform_clipboard` (function not defined).

**Step 3: Implement the minimal happy-path logic in `src/transform.rs`**

```rust
/// (original host, FxEmbed host) rewrite rules, in priority order.
const RULES: &[(&str, &str)] = &[
    ("twitter.com", "fxtwitter.com"),
    ("x.com", "fixupx.com"),
    ("bsky.app", "fxbsky.app"),
];

/// If `text` is a single X/Twitter/Bluesky URL, return the FxEmbed form.
/// Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split off the scheme (preserve it for the output). Only https for now.
    let (scheme, after_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https://", rest)
    } else {
        // No recognised scheme -> treat the whole string as the host+path.
        ("", trimmed)
    };

    // Host = everything up to the first '/' (or the whole thing if no path).
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = &after_scheme[..path_start];

    for &(domain, replacement) in RULES {
        if host == domain {
            let new_host = host.replacen(domain, replacement, 1);
            let mut result = String::with_capacity(trimmed.len() + 4);
            result.push_str(scheme);
            result.push_str(&new_host);
            result.push_str(&after_scheme[path_start..]);
            return Some(result);
        }
    }
    None
}
```

**Step 4: Run tests to verify pass**

Run:
```bash
cargo test --test transform
```
Expected: PASS — `3 passed`.

**Step 5: Commit**

```bash
git add tests/transform.rs src/transform.rs
git commit -m "feat: rewrite twitter/x/bsky https links to fxembed form"
```

---

## Task 3: Transform logic — scheme/subdomain/path coverage (TDD)

**Objective:** Extend the transform to handle `http://`, no-scheme, subdomains (`mobile.twitter.com`), trailing path/query, and surrounding whitespace. Each new test fails on the Task 2 impl first.

**Files:**
- Modify: `tests/transform.rs`
- Modify: `src/transform.rs`

**Step 1: Add failing tests** (append to `tests/transform.rs`)

```rust
#[test]
fn transforms_http_scheme() {
    assert_eq!(
        transform_clipboard("http://twitter.com/foo"),
        Some("http://fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transforms_no_scheme() {
    assert_eq!(
        transform_clipboard("twitter.com/foo"),
        Some("fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transforms_subdomain() {
    assert_eq!(
        transform_clipboard("https://mobile.twitter.com/foo"),
        Some("https://mobile.fxtwitter.com/foo".to_string())
    );
}

#[test]
fn transforms_x_subdomain() {
    assert_eq!(
        transform_clipboard("https://api.x.com/2/foo"),
        Some("https://api.fixupx.com/2/foo".to_string())
    );
}

#[test]
fn preserves_query_and_fragment() {
    assert_eq!(
        transform_clipboard("https://twitter.com/foo?s=123&t=abc#ref"),
        Some("https://fxtwitter.com/foo?s=123&t=abc#ref".to_string())
    );
}

#[test]
fn transforms_bare_host_no_path() {
    assert_eq!(
        transform_clipboard("https://twitter.com"),
        Some("https://fxtwitter.com".to_string())
    );
}

#[test]
fn trims_surrounding_whitespace() {
    assert_eq!(
        transform_clipboard("  https://twitter.com/foo\n"),
        Some("https://fxtwitter.com/foo".to_string())
    );
}
```

**Step 2: Run tests to verify which fail**

Run:
```bash
cargo test --test transform
```
Expected: `transforms_http_scheme` FAILS (Task 2 impl strips only `https://`, so the scheme is dropped -> returns `fxtwitter.com/foo` without `http://`). `transforms_subdomain` and `transforms_x_subdomain` FAIL (host `mobile.twitter.com` != `twitter.com` exact match -> returns `None`). The path/query/trim/bare-host/no-scheme tests already pass.

**Step 3: Extend the implementation in `src/transform.rs`**

Replace the scheme-split block and the host-match condition. Full updated file:

```rust
/// (original host, FxEmbed host) rewrite rules, in priority order.
const RULES: &[(&str, &str)] = &[
    ("twitter.com", "fxtwitter.com"),
    ("x.com", "fixupx.com"),
    ("bsky.app", "fxbsky.app"),
];

/// True if `host` is exactly `domain` or a subdomain of it (`*.domain`).
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{}", domain))
}

/// If `text` is a single X/Twitter/Bluesky URL, return the FxEmbed form.
/// Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split off the scheme (preserve it for the output).
    let (scheme, after_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https://", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http://", rest)
    } else {
        ("", trimmed)
    };

    // Host = everything up to the first '/' (or the whole thing if no path).
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = &after_scheme[..path_start];

    for &(domain, replacement) in RULES {
        if host_matches(host, domain) {
            let new_host = host.replacen(domain, replacement, 1);
            let mut result = String::with_capacity(trimmed.len() + 4);
            result.push_str(scheme);
            result.push_str(&new_host);
            result.push_str(&after_scheme[path_start..]);
            return Some(result);
        }
    }
    None
}
```

**Step 4: Run tests to verify pass**

Run:
```bash
cargo test --test transform
```
Expected: PASS — `10 passed` (3 from Task 2 + 7 new).

**Step 5: Commit**

```bash
git add tests/transform.rs src/transform.rs
git commit -m "feat: handle http, no-scheme, subdomains, query/fragment in transform"
```

---

## Task 4: Transform logic — skip guards (TDD)

**Objective:** Ensure already-transformed links, non-matching URLs, plain prose, empty input, and URLs with internal whitespace are all left alone. The whitespace guard is new behaviour; the rest are regression tests that should already pass and must keep passing.

**Files:**
- Modify: `tests/transform.rs`
- Modify: `src/transform.rs`

**Step 1: Add failing tests** (append to `tests/transform.rs`)

```rust
#[test]
fn skips_already_transformed_twitter() {
    assert_eq!(transform_clipboard("https://fxtwitter.com/foo"), None);
}

#[test]
fn skips_already_transformed_x() {
    assert_eq!(transform_clipboard("https://fixupx.com/foo"), None);
}

#[test]
fn skips_already_transformed_bsky() {
    assert_eq!(transform_clipboard("https://fxbsky.app/foo"), None);
}

#[test]
fn skips_non_matching_url() {
    assert_eq!(transform_clipboard("https://example.com/foo"), None);
}

#[test]
fn skips_plain_text() {
    assert_eq!(transform_clipboard("Hello world"), None);
}

#[test]
fn skips_empty() {
    assert_eq!(transform_clipboard(""), None);
    assert_eq!(transform_clipboard("   "), None);
}

#[test]
fn skips_url_with_internal_space() {
    // Not a clean URL -> leave alone.
    assert_eq!(transform_clipboard("https://twitter.com/foo bar"), None);
}

#[test]
fn skips_prose_containing_url() {
    assert_eq!(transform_clipboard("check out https://twitter.com/foo"), None);
}
```

**Step 2: Run tests to verify which fail**

Run:
```bash
cargo test --test transform
```
Expected: `skips_url_with_internal_space` FAILS (Task 3 impl matches host `twitter.com` and returns `Some("https://fxtwitter.com/foo bar")`). `skips_prose_containing_url` already passes (no scheme -> `after_scheme` = whole prose, host = `check out https:` which matches no rule). The already-transformed skips already pass because the prefixed hosts (`fxtwitter.com` etc.) do not equal nor end with `.twitter.com`/`.x.com`/`.bsky.app`.

**Step 3: Add the whitespace guard in `src/transform.rs`**

Insert this check immediately after the `trimmed.is_empty()` guard (before the scheme split):

```rust
    // Only rewrite clean single URLs (no internal whitespace/tabs).
    // URLs never contain spaces; prose does.
```

And the guard itself — add right after computing `after_scheme` (i.e. after the scheme-split block), before the host extraction:

```rust
    if after_scheme.contains(' ') || after_scheme.contains('\t') {
        return None;
    }
```

The final `src/transform.rs` after this task:

```rust
/// (original host, FxEmbed host) rewrite rules, in priority order.
const RULES: &[(&str, &str)] = &[
    ("twitter.com", "fxtwitter.com"),
    ("x.com", "fixupx.com"),
    ("bsky.app", "fxbsky.app"),
];

/// True if `host` is exactly `domain` or a subdomain of it (`*.domain`).
fn host_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{}", domain))
}

/// If `text` is a single X/Twitter/Bluesky URL, return the FxEmbed form.
/// Otherwise return `None` (leave the clipboard untouched).
pub fn transform_clipboard(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split off the scheme (preserve it for the output).
    let (scheme, after_scheme) = if let Some(rest) = trimmed.strip_prefix("https://") {
        ("https://", rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        ("http://", rest)
    } else {
        ("", trimmed)
    };

    // Only rewrite clean single URLs (no internal whitespace).
    if after_scheme.contains(' ') || after_scheme.contains('\t') {
        return None;
    }

    // Host = everything up to the first '/' (or the whole thing if no path).
    let path_start = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host = &after_scheme[..path_start];

    for &(domain, replacement) in RULES {
        if host_matches(host, domain) {
            let new_host = host.replacen(domain, replacement, 1);
            let mut result = String::with_capacity(trimmed.len() + 4);
            result.push_str(scheme);
            result.push_str(&new_host);
            result.push_str(&after_scheme[path_start..]);
            return Some(result);
        }
    }
    None
}
```

**Step 4: Run tests to verify pass**

Run:
```bash
cargo test --test transform
```
Expected: PASS — `18 passed` total.

**Step 5: Commit**

```bash
git add tests/transform.rs src/transform.rs
git commit -m "feat: skip already-transformed, prose, and whitespace-containing clipboard text"
```

---

## Task 5: Clipboard read/write wrapper (Win32)

**Objective:** Provide safe-ish wrappers over the Win32 clipboard text API (`CF_UNICODETEXT`) plus a UTF-16 string helper. Not unit-testable (needs a live clipboard session); verified by compile + a manual smoke test in Task 10.

**Files:**
- Modify: `src/clipboard.rs`

**Step 1: Implement `src/clipboard.rs`**

```rust
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
    if h == 0 {
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
    if h == 0 {
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
    SetClipboardData(CF_UNICODETEXT as u32, h) != 0
}
```

**Step 2: Build to verify it compiles**

Run:
```bash
cargo build
```
Expected: `Finished`. (If `CF_UNICODETEXT` is reported as a newtype rather than `u32`, the `as u32` casts already handle it. If `GMEM_MOVEABLE` is a newtype and `GlobalAlloc`'s first arg expects that newtype, `GMEM_MOVEABLE` is already that type — no cast needed.)

**Step 3: Commit**

```bash
git add src/clipboard.rs
git commit -m "feat: win32 clipboard read/write wrappers (CF_UNICODETEXT)"
```

---

## Task 6: Clipboard monitor — window, listener, message loop, write-on-event

**Objective:** Create the invisible message-only window, register it as a clipboard format listener, run the message loop, and on `WM_CLIPBOARDUPDATE` read → transform → defer-write the result back. No tray yet (Task 7/8); for now quitting is via the OS/process. This is the core of the app.

**Files:**
- Modify: `src/monitor.rs`

**Step 1: Implement `src/monitor.rs`**

```rust
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, DestroyWindow, GetMessageW,
    PostMessageW, PostQuitMessage, RegisterClassExW, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE, WM_DESTROY, WNDCLASSEXW, WNDPROC,
};
use windows_sys::core::PCWSTR;

use crate::clipboard;
use crate::transform::transform_clipboard;

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
        wc.lpszClassName = PCWSTR(class_name.as_ptr());

        if RegisterClassExW(&wc) == 0 {
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(clipboard::wide("AutoFxEmbed").as_ptr()),
            0,
            0, 0, 0, 0,
            HWND_MESSAGE,
            0,
            0,
            std::ptr::null(),
        );
        if hwnd == 0 {
            return;
        }

        // Event-driven clipboard monitoring: no polling thread, zero idle CPU.
        if AddClipboardFormatListener(hwnd) == 0 {
            DestroyWindow(hwnd);
            return;
        }

        // Tray icon added in Task 7 here: crate::tray::add(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Tray removal added in Task 7 here: crate::tray::remove(hwnd);
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
            LRESULT(0)
        }
        WM_DO_WRITE => {
            do_pending_write(hwnd);
            LRESULT(0)
        }
        // Tray callback (WM_APP+1) handled in Task 8.
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
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
    let Some(new_text) = transform_clipboard(&text) else {
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
```

**Step 2: Build to verify it compiles**

Run:
```bash
cargo build
```
Expected: `Finished`. 

> **windows-sys type-wrapper pitfall:** between versions, some `windows-sys` fields/constants are newtype wrappers (e.g. `WNDCLASS_STYLES`, `MENU_ITEM_FLAGS`) rather than raw `u32`. The zeroed-struct + named-field assignment pattern above avoids most of these. If the compiler flags a type mismatch on `wc.style` or a constant assignment, cast with `as u32` (e.g. `(CS_HREDRAW | CS_VREDRAW).0` or `as u32`) — this is cosmetic, not a logic change. If a constant like `WM_CLIPBOARDUPDATE` is not exported by your exact version, define it locally as `const WM_CLIPBOARDUPDATE: u32 = 0x031D;` and drop the import.

**Step 3: Commit**

```bash
git add src/monitor.rs
git commit -m "feat: event-driven clipboard monitor with deferred rewrite"
```

---

## Task 7: System tray — add/remove icon

**Objective:** Show a tray icon while the app runs and remove it on exit. Still no menu (Task 8); the icon at least gives visible presence.

**Files:**
- Modify: `src/tray.rs`
- Modify: `src/monitor.rs`

**Step 1: Implement `src/tray.rs`**

```rust
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{IDI_APPLICATION, LoadIconW};

/// Custom message Windows sends to our window when the tray icon is interacted with.
/// (WM_APP = 0x8000.)
pub const TRAY_CALLBACK_MSG: u32 = 0x8001;

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
    nid.hIcon = LoadIconW(0, IDI_APPLICATION);
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
```

**Step 2: Wire tray add/remove into `src/monitor.rs`**

In `run()`, replace the two placeholder comments:

- Replace `// Tray icon added in Task 7 here: crate::tray::add(hwnd);` with:
  ```rust
  crate::tray::add(hwnd);
  ```
- Replace `// Tray removal added in Task 7 here: crate::tray::remove(hwnd);` with:
  ```rust
  crate::tray::remove(hwnd);
  ```

**Step 3: Build**

Run:
```bash
cargo build
```
Expected: `Finished`.

**Step 4: Commit**

```bash
git add src/tray.rs src/monitor.rs
git commit -m "feat: system tray icon presence (add/remove)"
```

---

## Task 8: System tray — right-click "Quit" menu

**Objective:** Handle the tray callback message; on right-click show a popup menu with a "Quit" item that posts `WM_QUIT` so the app exits cleanly.

**Files:**
- Modify: `src/tray.rs`
- Modify: `src/monitor.rs`

**Step 1: Add the menu handler to `src/tray.rs`**

Append to `src/tray.rs` (add the new imports to the existing `use` blocks at the top, then the function):

Add to the `windows_sys::Win32::UI::WindowsAndMessaging` import list:
```rust
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, TrackPopupMenu, MF_STRING,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_TOPALIGN, WM_RBUTTONUP,
```
Add to the `windows_sys::Win32::Foundation` import:
```rust
    LPARAM, POINT,
```
Add `windows_sys::core::PCWSTR` import:
```rust
use windows_sys::core::PCWSTR;
```
Add the menu-id constant near `TRAY_CALLBACK_MSG`:
```rust
const MENU_QUIT: u32 = 1;
```

Then append the handler function:

```rust
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
    if menu == 0 {
        return;
    }
    AppendMenuW(
        menu,
        MF_STRING,
        MENU_QUIT as usize,
        PCWSTR(crate::clipboard::wide("Quit").as_ptr()),
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

    if cmd == MENU_QUIT as usize {
        windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
    }
}
```

**Step 2: Dispatch the tray callback in `src/monitor.rs` window_proc**

In `window_proc`, replace the placeholder comment `// Tray callback (WM_APP+1) handled in Task 8.` with:

```rust
        crate::tray::TRAY_CALLBACK_MSG => {
            crate::tray::handle_event(hwnd, lparam);
            LRESULT(0)
        }
```

**Step 3: Build**

Run:
```bash
cargo build
```
Expected: `Finished`.

**Step 4: Commit**

```bash
git add src/tray.rs src/monitor.rs
git commit -m "feat: tray right-click Quit menu"
```

---

## Task 9: Wire main + full dev build + end-to-end manual test

**Objective:** Confirm `main.rs` already calls `monitor::run()` (it does from Task 1), build the dev binary, and run a real end-to-end test: copy a tweet link, watch the clipboard become the FxEmbed link.

**Files:** none changed (main.rs already correct from Task 1).

**Step 1: Run the full test suite**

Run:
```bash
cargo test
```
Expected: the 18 transform tests PASS. (The lib compiles the windows-sys modules but the tests only exercise `transform_clipboard`, so no clipboard APIs are called during tests.)

**Step 2: Build the dev binary**

Run:
```bash
cargo build
```
Expected: `Finished`. Binary at `target/debug/autofxembed.exe`.

**Step 3: Manual end-to-end test**

1. Launch the app (it starts with no console window; a tray icon appears):
   ```bash
   ./target/debug/autofxembed.exe &
   ```
   (The `&` backgrounds it so you keep your shell. The tray icon uses the standard Windows application icon.)
2. Copy a tweet link to the clipboard. From git-bash you can set the clipboard text with PowerShell via `powershell.exe` (git-bash has no native clipboard tool):
   ```bash
   echo -n "https://twitter.com/elonmusk/status/1234567890" | clip.exe
   ```
   (`clip.exe` is a built-in Windows tool that copies stdin to the clipboard.)
3. Read the clipboard back to verify it was rewritten:
   ```bash
   powershell.exe -NoProfile -Command "Get-Clipboard"
   ```
   Expected output: `https://fxtwitter.com/elonmusk/status/1234567890`
4. Repeat for X and Bluesky:
   ```bash
   echo -n "https://x.com/elonmusk/status/123" | clip.exe
   powershell.exe -NoProfile -Command "Get-Clipboard"   # -> https://fixupx.com/elonmusk/status/123
   echo -n "https://bsky.app/profile/did.example" | clip.exe
   powershell.exe -NoProfile -Command "Get-Clipboard"   # -> https://fxbsky.app/profile/did.example
   ```
5. Negative test — a non-link must NOT be changed:
   ```bash
   echo -n "Hello world" | clip.exe
   powershell.exe -NoProfile -Command "Get-Clipboard"   # -> Hello world  (unchanged)
   ```
6. Negative test — an already-transformed link must NOT be double-rewritten:
   ```bash
   echo -n "https://fxtwitter.com/foo" | clip.exe
   powershell.exe -NoProfile -Command "Get-Clipboard"   # -> https://fxtwitter.com/foo  (unchanged, no loop)
   ```
7. Quit via the tray icon: right-click the AutoFxEmbed tray icon → "Quit". The process exits and the tray icon disappears.

**Step 4: Commit (no code changes, but mark the milestone)**

```bash
git add -A
git commit --allow-empty -m "test: end-to-end manual verification passes"
```

---

## Task 10: Release build, binary size, README

**Objective:** Produce a stripped, size-optimised release binary and document usage + build.

**Files:**
- Create: `README.md`

**Step 1: Build release**

Run:
```bash
cargo build --release
```
Expected: `Finished`. Binary at `target/release/autofxembed.exe`.

**Step 2: Check binary size**

Run:
```bash
ls -lh target/release/autofxembed.exe
```
Expected: a small binary (target: a few hundred KB). With `opt-level="z"` + LTO + `panic="abort"` + strip, a `windows-sys`-only app like this typically lands well under ~500 KB. (Exact size varies by toolchain.)

**Step 3: Repeat the Task 9 end-to-end test with the release binary**

```bash
./target/release/autofxembed.exe &
echo -n "https://twitter.com/u/status/1" | clip.exe
powershell.exe -NoProfile -Command "Get-Clipboard"   # -> https://fxtwitter.com/u/status/1
```
Then right-click tray → Quit.

**Step 4: Create `README.md`**

```markdown
# AutoFxEmbed

A tiny Windows background utility that watches your clipboard and rewrites
X / Twitter / Bluesky links into their [FxEmbed](https://github.com/FxEmbed/FxEmbed)
form so Discord, Telegram, etc. embed them properly.

## What it does

When the clipboard contains a single link to one of:

| Original        | Rewritten to     |
|-----------------|------------------|
| `twitter.com`   | `fxtwitter.com`  |
| `x.com`         | `fixupx.com`     |
| `bsky.app`      | `fxbsky.app`     |

…everything else in the URL (subdomain, path, query, fragment) is preserved and
the clipboard is updated in place. Already-transformed links, prose, and
non-matching URLs are left untouched.

It uses the Win32 clipboard format listener, so it wakes only on real clipboard
changes — no polling, no CPU at idle. A system-tray icon provides a right-click
"Quit" menu.

## Build

Requires the Rust toolchain with the MSVC linker (`rustup default stable-x86_64-pc-windows-msvc`).

```bash
cargo build --release
```

The binary is `target/release/autofxembed.exe`.

## Run

Double-click `autofxembed.exe` (no console window appears; a tray icon does).
Copy a tweet/X/Bluesky link; paste it anywhere — it's already the FxEmbed form.
Right-click the tray icon → "Quit" to exit.

## Tests

```bash
cargo test
```
(Unit tests cover the pure URL-rewrite logic; the Win32 glue is verified manually.)
```

**Step 5: Commit**

```bash
git add README.md
git commit -m "docs: README with build/run/test instructions"
```

---

## Files likely to change (summary)

| File             | Purpose                                      |
|------------------|----------------------------------------------|
| `Cargo.toml`     | Deps (`windows-sys` 0.61) + release profile  |
| `src/lib.rs`     | Module re-exports                            |
| `src/transform.rs` | Pure URL rewrite (fully unit-tested)       |
| `src/clipboard.rs` | Win32 clipboard read/write + wide helper   |
| `src/monitor.rs` | Message-only window, listener, message loop  |
| `src/tray.rs`    | Tray icon + right-click Quit menu            |
| `src/main.rs`    | GUI subsystem + `monitor::run()`             |
| `tests/transform.rs` | TDD integration tests for transform     |
| `README.md`      | Usage + build docs                           |

## Tests / validation

- **Automated:** `cargo test` → 18 tests in `tests/transform.rs` covering happy path, `http://`, no-scheme, subdomains, query/fragment, bare host, trim, already-transformed skip, non-matching URL skip, plain-text skip, empty skip, internal-whitespace skip, prose skip.
- **Manual (Task 9 & 10):** copy each link type to the clipboard, read it back via `powershell.exe -NoProfile -Command "Get-Clipboard"`, confirm the rewrite; confirm negatives are unchanged; confirm tray Quit exits cleanly.

## Risks, tradeoffs, and open questions

- **windows-sys version drift:** feature names are stable across 0.52–0.61, but field types are sometimes newtype wrappers. The zeroed-struct + named-field pattern mitigates this; a pitfall note in Task 6 covers the remaining casts.
- **Clipboard re-entrancy:** handled by deferring the write via `PostMessageW(WM_DO_WRITE)` so we never call `OpenClipboard` inside the `WM_CLIPBOARDUPDATE` broadcast.
- **Infinite loop:** prevented structurally — after we write `fxtwitter.com/…`, the next `WM_CLIPBOARDUPDATE` reads it and `transform_clipboard` returns `None` (the prefixed host doesn't match any rule), so no second write. No explicit "last-seen" state is needed.
- **No custom icon:** uses `IDI_APPLICATION` (standard Windows icon) to avoid embedding a `.ico` resource. A custom icon can be added later by embedding a resource via a `.rc`/`embed-resource` — out of scope for v1 (YAGNI).
- **No logging:** silent by design (resource-friendly, no console). If debugging is needed, temporarily comment out `#![windows_subsystem = "windows"]` in `main.rs` and add `eprintln!` calls.
- **Single-platform:** Windows-only by design (Win32 clipboard listener + tray). The pure `transform` module is platform-independent and unit-tests run anywhere, but the binary targets `x86_64-pc-windows-msvc`.
- **`panic = "abort"` in release:** shrinks the binary but means `cargo test` must use the dev profile (it does by default) — release tests are not run.
- **`OpenClipboard` can fail** if another app holds the clipboard; we simply skip that event (return `None`) and try again on the next change. Acceptable for a best-effort clipboard tool.
- **Open question — should it also transform when the clipboard holds rich content / HTML?** v1 only handles `CF_UNICODETEXT` (plain text). If you copy a link from a rich source you usually still get the URL as text, so this covers the common case. HTML-clipboard support can be added later if needed.
