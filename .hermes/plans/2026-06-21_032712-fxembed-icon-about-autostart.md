# AutoFxEmbed — FxEmbed icon + bold About + Start-on-startup Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Replace the tray's default Windows icon with the FxEmbed logo, add a bold "About" menu item that shows a message box, and add a checkable "Start on startup" toggle that writes/removes a Windows Run-key registry value.

**Architecture:** The app stays a pure `windows-sys` Win32 message-loop app. Three additive changes: (1) embed `assets/fxembed.ico` as a Windows resource via a `build.rs` + `embed-resource` + hand-authored `icon.rc` (deterministic resource ID 1), loaded at runtime with `LoadImageW`; (2) a new `src/autostart.rs` module that reads/writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; (3) `src/tray.rs` gains the new menu items (checkable toggle, bold About via `MF_DEFAULT`, a `MessageBoxW`). All new code is Win32 glue — manual-only verification, consistent with the existing project convention (pure URL-rewrite logic is the only auto-tested code).

**Tech Stack:** Rust 2021, `windows-sys` 0.61 (new features: `Win32_System_Registry`, `Win32_System_LibraryLoader`), `embed-resource` 2.x build-dependency, Windows SDK `rc.exe` (already installed at `C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\`), Pillow 12.2.0 (already installed) for ICO assembly, optional `resvg` for SVG rasterization.

---

## Current context / verified facts

- Project: `Cargo.toml` — `autofxembed` lib+bin (has `src/lib.rs`, so `cargo test` works and runs `tests/transform.rs` = 18 tests). `src/main.rs` is `#![windows_subsystem = "windows"]` calling `autofxembed::monitor::run()`.
- `src/tray.rs`: `add(hwnd)` sets `nid.hIcon = LoadIconW(null, IDI_APPLICATION)` (the default app icon — to be replaced). `handle_event` builds a popup with a single `MF_STRING` "Quit" item (`MENU_QUIT = 1`) and calls `PostQuitMessage(0)` on click.
- `src/clipboard.rs` has `pub(crate) fn wide(s: &str) -> Vec<u16>` (null-terminated UTF-16) — reused (DRY) by the new autostart module and tray.
- `src/monitor.rs` calls `crate::tray::{add, remove, handle_event}` and matches `TRAY_CALLBACK_MSG` — **unchanged** by this plan (signatures preserved).
- **Verified windows-sys 0.61.2 API facts** (from the crate source in the cargo cache):
  - `HICON = *mut c_void`, `HANDLE = *mut c_void`, `HMODULE = *mut c_void`, `HINSTANCE = *mut c_void` (all identical pointer type → `LoadImageW` return assigns directly to `NOTIFYICONDATAW.hIcon`).
  - `PCWSTR = *const u16` (bare alias → `1u16 as usize as PCWSTR` is the manual `MAKEINTRESOURCEW(1)`; **`MAKEINTRESOURCEW` is NOT exposed by windows-sys 0.61.2**).
  - `LoadImageW(hinst: HINSTANCE, name: PCWSTR, type: GDI_IMAGE_TYPE, cx: i32, cy: i32, fuload: IMAGE_FLAGS) -> HANDLE`; `IMAGE_ICON = 1`, `LR_DEFAULTSIZE = 64`, `LR_SHARED = 32768`.
  - `GetModuleHandleW(lpmodulename: PCWSTR) -> HMODULE` and `GetModuleFileNameW(hmodule: HMODULE, lpfilename: PWSTR, nsize: u32) -> u32` — both in `Win32::System::LibraryLoader` (feature `Win32_System_LibraryLoader`).
  - Registry fns in `Win32::System::Registry` (feature `Win32_System_Registry`): `RegOpenKeyExW`, `RegQueryValueExW`, `RegSetValueExW`, `RegDeleteValueW`, `RegCloseKey`; `HKEY = *mut c_void`; `HKEY_CURRENT_USER`, `KEY_READ = 131097`, `KEY_SET_VALUE = 2`, `REG_SZ = 1`.
  - Menu/MB constants in `Win32::UI::WindowsAndMessaging` (already enabled): `MF_CHECKED = 8`, `MF_DEFAULT = 4096` (renders bold — the default menu item), `MF_SEPARATOR = 2048`, `MF_UNCHECKED = 0`, `MF_STRING` (already used); `MessageBoxW(hwnd, text: PCWSTR, caption: PCWSTR, utype: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT`; `MB_OK = 0`, `MB_ICONINFORMATION = 64`.
  - `ERROR_SUCCESS = 0`, `ERROR_FILE_NOT_FOUND = 2` in `Win32::Foundation` (already enabled).
- **FxEmbed asset facts** (from the GitHub API): `fxembed.svg` exists at `assets/logos/fxembed.svg` but has **no pre-made PNG variants** (unlike fxtwitter/fixupx/fxbluesky which have 16/24/32/48/64 PNGs). A ready raster `fxembedpfp.png` (the FxEmbed profile picture) exists and is a usable fallback.
- **Host tools**: Pillow 12.2.0 installed. No ImageMagick/Inkscape/resvg/cairosvg installed (the `convert` on PATH is Windows' NTFS `convert.exe`, not ImageMagick). `rc.exe` present in the Windows SDK. `cargo`/`rustc` at `~/.cargo/bin` — **not on default git-bash PATH**, so `export PATH="$HOME/.cargo/bin:$PATH"` before every cargo command.

## Testing strategy

All new code is Win32 glue (icon loading, registry, menu, message box) — **manual-only verification**, matching the project's existing convention (the README states: "Unit tests cover the pure URL-rewrite logic; the Win32 glue is verified manually."). No new automated tests are added. After every code task, run `cargo test` to confirm the 18 existing transform tests still pass (regression guard), then `cargo build` to confirm compilation, then the task's manual verification.

---

## Task 1: Generate `assets/fxembed.ico` from the FxEmbed SVG

**Objective:** Produce a committed multi-size `.ico` (16/24/32/48/64/256) of the FxEmbed logo that later tasks embed as a Windows resource.

**Files:**
- Create: `assets/fxembed.svg` (downloaded source, kept for attribution/reproducibility)
- Create: `assets/fxembed.ico` (generated, committed)
- Create (intermediate, optional to commit): `assets/fxembed_256.png`

**Step 1: Download the source SVG**

Run (from repo root, git-bash):
```bash
mkdir -p assets
curl -L -sS -o assets/fxembed.svg https://raw.githubusercontent.com/FxEmbed/FxEmbed/main/assets/logos/fxembed.svg
ls -l assets/fxembed.svg   # expect ~2704 bytes
```

**Step 2: Rasterize to a 256×256 PNG**

Get an SVG rasterizer. Recommended (deterministic, host has cargo):
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo install resvg    # installs the `resvg` CLI to ~/.cargo/bin (one-time, ~3-5 min compile)
```
*(Faster alternative: download the prebuilt Windows binary from https://github.com/RazrFalcon/resvg/releases — pick the `*-win64.zip`, extract `resvg.exe`.)*

Then rasterize:
```bash
resvg assets/fxembed.svg assets/fxembed_256.png -w 256
```
Verify it is square (needed for clean ICO sizing):
```bash
python -c "from PIL import Image; print(Image.open('assets/fxembed_256.png').size)"
```
Expected: `(256, 256)`. If the height differs (non-square viewBox), re-render with both dims: `resvg assets/fxembed.svg assets/fxembed_256.png -w 256 -h 256`.

**Fallback (if no rasterizer can be obtained):** use the ready-made FxEmbed profile-picture raster instead:
```bash
curl -L -sS -o assets/fxembed_pfp.png https://raw.githubusercontent.com/FxEmbed/FxEmbed/main/assets/logos/fxembedpfp.png
```
Then in Step 3 use `assets/fxembed_pfp.png` as the source. (Note: `fxembedpfp.png` is the logo rendered as a profile picture — it reads well at small tray sizes, but it is a different file from the SVG the user specified. Prefer the SVG rasterization; use this only if a rasterizer is unavailable.)

**Step 3: Assemble the multi-size ICO with Pillow**

Run:
```bash
python - <<'PY'
from PIL import Image
src = "assets/fxembed_256.png"
import os
if not os.path.exists(src):
    src = "assets/fxembed_pfp.png"  # fallback
im = Image.open(src).convert("RGBA")
if im.size != (256, 256):
    im = im.resize((256, 256), Image.Resampling.LANCZOS)
im.save(
    "assets/fxembed.ico",
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (256, 256)],
)
print("wrote assets/fxembed.ico")
PY
```

**Step 4: Verify the ICO**

```bash
python -c "from PIL import Image; im=Image.open('assets/fxembed.ico'); print(im.format, im.size)"
```
Expected: `ICO (256, 256)`. Then eyeball it: open `assets/fxembed.ico` in Windows Explorer (it shows the FxEmbed logo). At 16px the bare-SVG logo may look thin on a transparent background — if so, the `fxembedpfp.png` fallback (Step 2) gives a more visible small icon; pick whichever the user prefers and re-run Step 3.

**Step 5: Commit**

```bash
git add assets/fxembed.svg assets/fxembed.ico
git commit -m "chore: add FxEmbed icon asset (svg + ico)"
```

---

## Task 2: Add Cargo.toml features + `embed-resource` build-dep + `build.rs` + `icon.rc`

**Objective:** Embed `assets/fxembed.ico` into the `.exe` as Windows icon resource ID 1, so the runtime can load it with `LoadImageW` and Explorer shows the FxEmbed icon for the `.exe`.

**Files:**
- Modify: `Cargo.toml` (add 2 windows-sys features + `[build-dependencies]`)
- Create: `icon.rc` (repo root)
- Create: `build.rs` (repo root)

**Step 1: Edit `Cargo.toml`**

Current `[dependencies]` block:
```toml
[dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_DataExchange",
    "Win32_System_Ole",
    "Win32_System_Memory",
    "Win32_UI_Shell",
    "Win32_Graphics_Gdi",
] }
```
Replace with (adds `Win32_System_Registry`, `Win32_System_LibraryLoader`) and add a `[build-dependencies]` section immediately after the `[dependencies]` block:
```toml
[dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_System_DataExchange",
    "Win32_System_Ole",
    "Win32_System_Memory",
    "Win32_System_Registry",
    "Win32_System_LibraryLoader",
    "Win32_UI_Shell",
    "Win32_Graphics_Gdi",
] }

[build-dependencies]
embed-resource = "2"
```
Leave `[profile.release]` and everything else unchanged.

**Step 2: Create `icon.rc` (repo root)**

Full contents:
```
1 ICON "assets/fxembed.ico"
```
This pins the icon to resource ID `1` (RT_GROUP_ICON), which `LoadImageW` will request in Task 4. `rc.exe` resolves `"assets/fxembed.ico"` relative to the `.rc` file's directory (repo root), where `assets/fxembed.ico` exists from Task 1.

**Step 3: Create `build.rs` (repo root)**

Full contents:
```rust
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        embed_resource::compile("icon.rc", embed_resource::NONE);
    }
    println!("cargo:rerun-if-changed=icon.rc");
    println!("cargo:rerun-if-changed=assets/fxembed.ico");
}
```
`embed_resource::compile` invokes `rc.exe` (found in the installed Windows SDK). It panics with a clear message if `rc.exe` is missing or compilation fails — preferred over silently shipping an icon-less binary.

**Step 4: Build and verify**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
```
Expected: `Finished` (first run fetches the `embed-resource` crate from crates.io — requires network once; subsequent builds are offline). Then verify the `.exe` carries the icon — in Explorer, `target/release/autofxembed.exe` should display the FxEmbed logo as its file icon (or right-click → Properties → the icon in the top-left is the FxEmbed logo).
```bash
cargo test
```
Expected: `18 passed`.

**Step 5: Commit**

```bash
git add Cargo.toml icon.rc build.rs
git commit -m "build: embed FxEmbed icon as Windows resource (embed-resource + rc)"
```

---

## Task 3: Create `src/autostart.rs` scaffold + register the module

**Objective:** Add an empty-but-compiling `autostart` module so the project builds with the module wired into `lib.rs`; Task 5 fills in the real registry logic.

**Files:**
- Create: `src/autostart.rs`
- Modify: `src/lib.rs` (add one line)

**Step 1: Create `src/autostart.rs` (scaffold)**

Full contents:
```rust
//! Windows "Run" registry key support for start-on-startup (scaffold —
//! real bodies in a later task).

pub unsafe fn is_enabled() -> bool {
    false
}

pub unsafe fn toggle() -> bool {
    false
}
```
These stubs are `pub` (no dead-code warning) and contain no unsafe operations (no warning under the project's default lints). Nothing calls them yet — the module just needs to exist and compile.

**Step 2: Edit `src/lib.rs`**

Current:
```rust
pub mod transform;
pub mod clipboard;
pub mod monitor;
pub mod tray;
```
Add one line:
```rust
pub mod transform;
pub mod clipboard;
pub mod monitor;
pub mod tray;
pub mod autostart;
```

**Step 3: Build + test**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo test
```
Expected: `Finished` and `18 passed`.

**Step 4: Commit**

```bash
git add src/autostart.rs src/lib.rs
git commit -m "feat: scaffold autostart module"
```

---

## Task 4: Load the embedded FxEmbed icon for the tray

**Objective:** Replace `LoadIconW(IDI_APPLICATION)` with `LoadImageW` loading the embedded resource (ID 1), so the tray icon is the FxEmbed logo.

**Files:**
- Modify: `src/tray.rs` (imports + the `add()` icon line)

**Step 1: Update the imports in `src/tray.rs`**

Current import block (lines 5-9):
```rust
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IDI_APPLICATION, LoadIconW,
    MF_STRING, TrackPopupMenu, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_TOPALIGN,
    WM_RBUTTONUP,
};
```
Replace with (drop `IDI_APPLICATION, LoadIconW`; add `IMAGE_ICON, LoadImageW, LR_DEFAULTSIZE, LR_SHARED`; add the LibraryLoader import):
```rust
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IMAGE_ICON, LoadImageW,
    LR_DEFAULTSIZE, LR_SHARED, MF_STRING, TrackPopupMenu, TPM_LEFTALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_TOPALIGN, WM_RBUTTONUP,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
```

**Step 2: Replace the icon line in `add()`**

Current (line 28):
```rust
    nid.hIcon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
```
Replace with:
```rust
    let hinst = GetModuleHandleW(std::ptr::null());
    nid.hIcon = LoadImageW(
        hinst,
        1u16 as usize as windows_sys::core::PCWSTR,
        IMAGE_ICON,
        0,
        0,
        LR_DEFAULTSIZE | LR_SHARED,
    );
```
Type notes (verified): `GetModuleHandleW(null())` returns `HMODULE` (`*mut c_void`); `LoadImageW`'s `hinst` is `HINSTANCE` (`*mut c_void`) — identical type. `LoadImageW` returns `HANDLE` (`*mut c_void`); `nid.hIcon` is `HICON` (`*mut c_void`) — identical type, direct assignment. `1u16 as usize as PCWSTR` is the manual `MAKEINTRESOURCEW(1)` (PCWSTR is a bare `*const u16` alias; `MAKEINTRESOURCEW` is not exposed by windows-sys 0.61.2). `LR_SHARED` means the system owns the icon — do **not** `DestroyIcon` it.

**Step 3: Build + test**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo test
```
Expected: `Finished` and `18 passed`.

**Step 4: Manual verification**

Run `target/release/autofxembed.exe`. The system-tray icon should now be the FxEmbed logo (not the generic Windows app icon). If it shows the generic icon, the resource wasn't embedded with ID 1 — recheck Task 2's `icon.rc` (`1 ICON "assets/fxembed.ico"`) and that `assets/fxembed.ico` exists. Right-click → Quit to exit.

**Step 5: Commit**

```bash
git add src/tray.rs
git commit -m "feat: use embedded FxEmbed icon for the tray"
```

---

## Task 5: Implement the autostart registry logic

**Objective:** Fill `src/autostart.rs` with real registry read/toggle logic against `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

**Files:**
- Modify: `src/autostart.rs` (replace the scaffold with the full implementation)

**Step 1: Replace `src/autostart.rs` with the full implementation**

Full contents:
```rust
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
```

API signature notes (verified against windows-sys 0.61.2): `RegOpenKeyExW(hkey, lpsubkey: PCWSTR, uloptions: u32, samdesired, phkresult: *mut HKEY) -> WIN32_ERROR`; `&mut hkey` coerces `&mut (*mut c_void)` → `*mut HKEY`. `RegQueryValueExW(hkey, lpvaluename: PCWSTR, lpreserved: *const u32, lptype: *mut REG_VALUE_TYPE, lpdata: *mut u8, lpcbdata: *mut u32)`; `&mut ty` (ty: u32, REG_VALUE_TYPE = u32) coerces to `*mut u32`; `std::ptr::null()` for the reserved param infers `*const u32`. `RegSetValueExW(hkey, lpvaluename, reserved: u32, dwtype, lpdata: *const u8, cbdata: u32)`; `data.as_ptr() as *const u8` reinterprets the UTF-16 bytes. `GetModuleFileNameW(std::ptr::null_mut(), ...)` with `hmodule = NULL` returns the current exe's path. No `HMODULE` import is needed because `null_mut()` infers the pointer type.

**Step 2: Build + test**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo test
```
Expected: `Finished` and `18 passed`. (No automated tests for this module — Win32 glue, manual-only per project convention; verified end-to-end via the tray menu in Task 6.)

**Step 3: Commit**

```bash
git add src/autostart.rs
git commit -m "feat: implement start-on-startup via HKCU Run registry key"
```

---

## Task 6: Add the Start-on-startup toggle + bold About menu items

**Objective:** Build the right-click tray menu with three items (checkable Start-on-startup, bold About, Quit) and wire their commands (toggle registry / show MessageBox / quit).

**Files:**
- Modify: `src/tray.rs` (menu-item ID constants, imports, `handle_event` body)

**Step 1: Add menu-item ID constants**

Current (lines 15-16):
```rust
/// Menu item ID for "Quit".
const MENU_QUIT: u32 = 1;
```
Replace with:
```rust
/// Menu item IDs.
const MENU_QUIT: u32 = 1;
const MENU_STARTUP: u32 = 2;
const MENU_ABOUT: u32 = 3;
```

**Step 2: Extend the imports**

Current (set in Task 4):
```rust
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IMAGE_ICON, LoadImageW,
    LR_DEFAULTSIZE, LR_SHARED, MF_STRING, TrackPopupMenu, TPM_LEFTALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_TOPALIGN, WM_RBUTTONUP,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
```
Replace the first block with (add `MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_DEFAULT, MF_SEPARATOR, MessageBoxW`):
```rust
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, IMAGE_ICON, LoadImageW,
    LR_DEFAULTSIZE, LR_SHARED, MB_ICONINFORMATION, MB_OK, MF_CHECKED, MF_DEFAULT, MF_SEPARATOR,
    MF_STRING, MessageBoxW, TrackPopupMenu, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_TOPALIGN, WM_RBUTTONUP,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
```

**Step 3: Replace the `handle_event` body**

Current (lines 49-84):
```rust
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

    if cmd == MENU_QUIT as i32 {
        windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
    }
}
```
Replace with:
```rust
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
```
Notes: the menu is rebuilt on every right-click, so the Start-on-startup check is read fresh each time (no cached state). `MF_DEFAULT` renders "About" bold (only one default item is allowed — we have exactly one). The `<3` in the About text is the literal characters `<` and `3` (a heart emoticon). The `else { 0 }` arm of the `if` infers to `MENU_ITEM_FLAGS` (u32) to match `MF_CHECKED`, so `MF_STRING | (...)` type-checks.

**Step 4: Build + test**

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release
cargo test
```
Expected: `Finished` and `18 passed`.

**Step 5: Manual verification (the core acceptance test)**

Run `target/release/autofxembed.exe`.

1. **Tray icon** shows the FxEmbed logo (from Task 4).
2. Right-click the tray icon → a popup appears with, in order: **Start on startup** (no check initially), a separator, **About** (rendered **bold**), a separator, **Quit**.
3. Click **About** → a message box appears, title "About", text exactly `Made by Cris with much <3 for his friends`, an OK button and an information (ℹ) icon. Click OK to close.
4. Click **Start on startup** → the menu closes. Open `regedit.exe` → navigate to `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` → there is a value named `AutoFxEmbed` whose data is the full path to `autofxembed.exe` wrapped in double quotes (e.g. `"C:\Users\bbccris\Documents\Proyectos\AutoFXEmbed\target\release\autofxembed.exe"`).
5. Right-click the tray icon again → **Start on startup** now shows a check mark.
6. Click **Start on startup** again → the `AutoFxEmbed` value is removed from the registry; right-click again → no check mark.
7. Click **Quit** → the app exits (tray icon disappears).

(Optional, to fully confirm startup-launch: with the value enabled, sign out and back in to Windows — `autofxembed.exe` should start and show the tray icon. This is the real proof the Run key works; the regedit check is sufficient for verification.)

**Step 6: Commit**

```bash
git add src/tray.rs
git commit -m "feat: add Start-on-startup toggle and bold About to tray menu"
```

---

## Task 7: Update README

**Objective:** Document the new icon, About, and Start-on-startup features.

**Files:**
- Modify: `README.md`

**Step 1: Update the tray description (lines 21-23)**

Current:
```
It uses the Win32 clipboard format listener, so it wakes only on real clipboard
changes — no polling, no CPU at idle. A system-tray icon provides a right-click
"Quit" menu.
```
Replace with:
```
It uses the Win32 clipboard format listener, so it wakes only on real clipboard
changes — no polling, no CPU at idle. A system-tray icon (the FxEmbed logo)
provides a right-click menu with **Start on startup**, a bold **About**, and
**Quit**.
```

**Step 2: Update the Run section (lines 37-39)**

Current:
```
Double-click `autofxembed.exe` (no console window appears; a tray icon does).
Copy a tweet/X/Bluesky link; paste it anywhere — it's already the FxEmbed form.
Right-click the tray icon → "Quit" to exit.
```
Replace with:
```
Double-click `autofxembed.exe` (no console window appears; a tray icon does).
Copy a tweet/X/Bluesky link; paste it anywhere — it's already the FxEmbed form.
Right-click the tray icon for:

- **Start on startup** — checked when AutoFxEmbed will launch at Windows logon
  (toggles a value under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- **About** — shows a small message box.
- **Quit** — exits.
```

**Step 3: Add a build note after the `cargo build --release` block (after line 33)**

Insert after the line `The binary is \`target/release/autofxembed.exe\`.`:
```

Building embeds the FxEmbed icon as a Windows resource (`build.rs` via
`embed-resource`, which requires `rc.exe` from the Windows SDK — included with
Visual Studio Build Tools).
```

**Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document FxEmbed icon, About, and Start-on-startup"
```

---

## Files likely to change (summary)

| File | Action | Task |
|------|--------|------|
| `assets/fxembed.svg` | create (download) | 1 |
| `assets/fxembed.ico` | create (generate) | 1 |
| `Cargo.toml` | modify (features + build-dep) | 2 |
| `icon.rc` | create | 2 |
| `build.rs` | create | 2 |
| `src/autostart.rs` | create (scaffold) → replace (impl) | 3, 5 |
| `src/lib.rs` | modify (add `pub mod autostart;`) | 3 |
| `src/tray.rs` | modify (icon + menu) | 4, 6 |
| `README.md` | modify | 7 |

## Tests / validation

- **Automated (regression):** after every code task, `cargo test` → expect `18 passed` (the existing transform tests; this plan adds no new tests because all new code is Win32 glue, manual-only per project convention).
- **Build:** after every code task, `cargo build --release` → expect `Finished`.
- **Manual acceptance:** Task 6 Step 5 is the end-to-end acceptance test (icon, bold About with exact text, toggle writes/removes the Run-key value, Quit exits).

## Risks, tradeoffs, and open questions

- **Icon at 16px:** the bare `fxembed.svg` logo on a transparent background may look thin in the 16px tray size. The `fxembedpfp.png` fallback (Task 1 Step 2) renders the logo as a profile picture and is more visible at small sizes. **Open question for the user:** prefer the exact SVG raster or the pfp render? The plan defaults to the SVG (honoring the explicit link) with the pfp as fallback.
- **Task 1 rasterizer:** `cargo install resvg` is a one-time ~3-5 min compile; the prebuilt-binary download is faster but the asset filename varies by release. The pfp fallback guarantees an `.ico` is produced even if no rasterizer is available.
- **Task 2 resource embedding:** `embed-resource` needs `rc.exe` (present in the Windows SDK 10.0.26100.0 at `C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\`). First build fetches the `embed-resource` crate (network once). If `rc.exe` is ever not found, `embed_resource::compile` panics with a clear message — install the Windows SDK / VS Build Tools.
- **`MAKEINTRESOURCEW` not in windows-sys 0.61.2:** handled by the manual cast `1u16 as usize as PCWSTR` (verified: PCWSTR is a bare `*const u16` alias). The icon resource ID is pinned to `1` by `icon.rc`'s `1 ICON "..."`.
- **Registry path staleness:** if the user moves `autofxembed.exe`, the Run-key value still points to the old path. Expected behavior — re-toggle to refresh. User-level key (`HKCU`) — no admin rights needed.
- **Menu ordering:** the plan uses Start-on-startup / separator / About (bold) / separator / Quit. If the user wants About immediately adjacent to Quit (no separator), drop the second `AppendMenuW(menu, MF_SEPARATOR, ...)` in Task 6.
- **`MF_DEFAULT` bold:** only one menu item may be the default; exactly one (About) is marked. This is the standard Windows way to render a bold menu item.
- **No automated tests for new code:** consistent with the project's stated convention. The pure logic added (registry value name, menu IDs) is trivial; the meaningful behavior is Win32 I/O verified manually in Task 6.
