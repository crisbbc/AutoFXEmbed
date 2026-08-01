# AutoFxEmbed

A tiny Windows background utility that watches your clipboard and rewrites
X / Twitter / Bluesky links into their [FxEmbed](https://github.com/FxEmbed/FxEmbed)
form so Discord, Telegram, etc. embed them properly.

## What it does

When the clipboard contains one or more links to one of these services:

| Original        | Rewritten to     |
|-----------------|------------------|
| `twitter.com`   | `fxtwitter.com`  |
| `x.com`         | `fixupx.com`     |
| `bsky.app`      | `fxbsky.app`     |

…everything else in the URL (subdomain, optional port, path, query, fragment) is
preserved and the clipboard is updated in place. Already-transformed links,
non-matching URLs are left untouched; supported links embedded in prose are
rewritten in place.

On Windows it uses the clipboard format listener, so it wakes only on real
clipboard changes — no polling, no CPU at idle. On Linux it polls every 100 ms
for desktop compatibility. A system-tray icon (the FxEmbed logo)
provides a right-click menu with **Start on startup**, a bold **About**, and
**Quit**.

## Install

Install the latest version directly from this repository with Cargo:

```bash
cargo install --git https://github.com/crisbbc/AutoFXEmbed.git --locked
```

Cargo installs the `autofxembed` executable into its bin directory (usually
`~/.cargo/bin` on Linux/macOS or `%USERPROFILE%\\.cargo\\bin` on Windows). Run it
from there or make sure that directory is on your `PATH`.

## Build

Requires the Rust toolchain with the MSVC linker (`rustup default stable-x86_64-pc-windows-msvc`).

```bash
cargo build --release
```

The binary is `target/release/autofxembed.exe`.

Building embeds the FxEmbed icon as a Windows resource (`build.rs` via
`embed-resource`, which requires `rc.exe` from the Windows SDK — included with
Visual Studio Build Tools).

## Run

Double-click `autofxembed.exe` (no console window appears; a tray icon does).
On Linux, a missing D-Bus StatusNotifier tray is treated as a startup failure
rather than leaving an unmanageable background process running.
Copy a tweet/X/Bluesky link; paste it anywhere — it's already the FxEmbed form.
Right-click the tray icon for:

- **Start on startup** — checked when AutoFxEmbed will launch at Windows logon
  (toggles a value under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- **About** — shows a small message box.
- **Quit** — exits.

## Tests

```bash
cargo fmt -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

(Unit tests cover the pure URL-rewrite logic; platform integration remains
manual.)
