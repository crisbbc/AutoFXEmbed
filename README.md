# AutoFxEmbed

A tiny background utility for Windows and Linux (X11 and Wayland) that watches your clipboard and rewrites
X / Twitter / Bluesky links through [FxEmbed](https://github.com/FxEmbed/FxEmbed), and
Instagram links through [Instagram7](https://www.instagram7.com/), and TikTok links through
[fxTikTok](https://github.com/okdargy/fxtiktok) so Discord, Telegram, etc. embed them
properly.

## What it does

When the clipboard contains one or more links to one of these services:

| Original        | Rewritten to     |
|-----------------|------------------|
| `twitter.com`   | `fxtwitter.com`  |
| `x.com`         | `fixupx.com`     |
| `bsky.app`      | `fxbsky.app`     |
| `instagram.com` | `instagram7.com` |
| `tiktok.com`    | `tnktok.com`     |

X / Twitter links can instead go to `boypussyx.com`, `mpregx.com`, `yaoisex.com` or `faggotx.com`; pick the
target from the tray icon's **X / Twitter** submenu (the choice is remembered).

…everything else in the URL (subdomain, optional port, path, query, fragment) is
preserved and the clipboard is updated in place. Already-transformed links,
non-matching URLs are left untouched; supported links embedded in prose (including
ones wrapped in brackets or quotes, like `(https://x.com/…)`) are rewritten in place.

Change detection is event-driven wherever possible:

- **Windows** — the clipboard format listener; no polling, no CPU at idle.
- **X11** — XFixes selection-owner events plus a slow 5 s safety read.
- **Wayland** — data-control events (`ext-data-control` or `wlr-data-control`).
  The PRIMARY selection (middle-click paste) is rewritten too. On compositors
  without data-control (e.g. GNOME) it falls back to a 250 ms poll.

A system-tray icon (the FxEmbed logo) provides a right-click menu (see below).

## History

Every converted link is remembered (the last 100; copying one again moves it to
the top). Once a link is converted, the embed page is fetched once in the
background to read its title and description, so the **Recent** submenu shows
what each link is. The history is stored in
`<config dir>/autofxembed/history.json` (`~/.config/autofxembed/` on Linux).
That fetch is the only network request AutoFxEmbed makes; if it fails the entry
is just shown by its URL.

## Install

Install the latest version directly from this repository with Cargo:

```bash
cargo install --git https://github.com/crisbbc/AutoFXEmbed.git --locked
```

Cargo installs the `autofxembed` executable into its bin directory (usually
`~/.cargo/bin` on Linux/macOS or `%USERPROFILE%\\.cargo\\bin` on Windows). Run it
from there or make sure that directory is on your `PATH`.

## Build

```bash
cargo build --release
```

**Windows** — requires the Rust toolchain with the MSVC linker
(`rustup default stable-x86_64-pc-windows-msvc`). The binary is
`target/release/autofxembed.exe`. Building embeds the FxEmbed icon as a Windows
resource (`build.rs` via `embed-resource`, which requires `rc.exe` from the
Windows SDK — included with Visual Studio Build Tools).

**Linux** — the binary is `target/release/autofxembed`. A desktop with a
StatusNotifierItem tray is required (KDE Plasma, or GNOME with an AppIndicator
extension).

## Run

On Windows, double-click `autofxembed.exe` (no console window appears; a tray
icon does). On Linux, run `autofxembed`; a missing D-Bus StatusNotifier tray is
treated as a startup failure rather than leaving an unmanageable background
process running.
Copy an X/Twitter, Bluesky, Instagram, or TikTok link; paste it anywhere — it's already embed-friendly.
Right-click the tray icon for:

- **X / Twitter** — submenu to pick the target (FixUpX, BoyPussyX, MpregX, YaoiSex or FaggotX);
  the choice is remembered.
- **Instagram** / **TikTok** — informational (the host each is rewritten to).
- **Recent** — the last 10 converted links, labeled with their title and
  description; click one to copy its embed link again.
- **Clear history** — forgets every stored link.
- **Start on startup** — checked when AutoFxEmbed will launch at login. On
  Windows this toggles a value under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; on Linux it creates
  `~/.config/autostart/autofxembed.desktop`.
- **About** — shows a small message box (a desktop notification on Linux).
- **Quit** — exits.

## Tests

```bash
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

(Unit tests cover the pure URL-rewrite logic; platform integration remains
manual.)
