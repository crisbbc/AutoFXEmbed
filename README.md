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
changes — no polling, no CPU at idle. A system-tray icon (the FxEmbed logo)
provides a right-click menu with **Start on startup**, a bold **About**, and
**Quit**.

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
Copy a tweet/X/Bluesky link; paste it anywhere — it's already the FxEmbed form.
Right-click the tray icon for:

- **Start on startup** — checked when AutoFxEmbed will launch at Windows logon
  (toggles a value under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- **About** — shows a small message box.
- **Quit** — exits.

## Tests

```bash
cargo test
```
(Unit tests cover the pure URL-rewrite logic; the Win32 glue is verified manually.)
