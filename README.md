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
