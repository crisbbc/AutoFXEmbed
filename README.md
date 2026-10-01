<p align="center">
  <img src="assets/fxembed.svg" alt="AutoFxEmbed logo" width="96" height="96">
</p>

<h1 align="center">AutoFxEmbed</h1>

<p align="center">
  A tiny tray utility that rewrites social media links on your clipboard so
  they embed properly in Discord, Telegram and friends.
</p>

<p align="center">
  <a href="https://github.com/crisbbc/AutoFXEmbed/actions/workflows/ci.yml">
    <img src="https://github.com/crisbbc/AutoFXEmbed/actions/workflows/ci.yml/badge.svg" alt="CI status">
  </a>
</p>

<p align="center">
  <a href="#install">Install</a> &middot;
  <a href="#usage">Usage</a> &middot;
  <a href="#tray-menu">Tray menu</a> &middot;
  <a href="#history">History</a> &middot;
  <a href="#build">Build</a>
</p>

---

AutoFxEmbed runs in the background on Windows and Linux (X11 and Wayland). It
watches the clipboard, and when you copy an X / Twitter, Bluesky or TikTok link
it swaps in an embed-friendly host through
[FxEmbed](https://github.com/FxEmbed/FxEmbed) or
[fxTikTok](https://github.com/okdargy/fxtiktok). Paste it anywhere and it just
works.

## Supported sites

| Service       | Original        | Rewritten to     |
|---------------|-----------------|------------------|
| X / Twitter   | `twitter.com`   | `fxtwitter.com`  |
| X / Twitter   | `x.com`         | `fixupx.com`     |
| Bluesky       | `bsky.app`      | `fxbsky.app`     |
| TikTok        | `tiktok.com`    | `tnktok.com`     |

- Everything else in the URL is preserved: subdomain, port, path, query and
  fragment. The clipboard is updated in place.
- Links embedded in prose are rewritten too, including ones wrapped in
  brackets or quotes, like `(https://x.com/...)`.
- Already-converted links and unrelated URLs are left untouched.

### Alternative X / Twitter targets

X / Twitter links can go to a different embed host. Pick one from the tray
icon's **X / Twitter** submenu; the choice is remembered.

| Target    | Rewritten to                             |
|-----------|------------------------------------------|
| FixUpX    | `fxtwitter.com` / `fixupx.com` (default) |
| BoyPussyX | `boypussyx.com`                          |
| MpregX    | `mpregx.com`                             |
| YaoiSex   | `yaoisex.com`                            |
| FaggotX   | `faggotx.com`                            |

> [!WARNING]
> Some of these alternative domains have names that may be offensive or NSFW.
> They are third-party services that I don't own, operate or control, and they
> are included only as optional targets. I'm not responsible for those
> domains, their names, their content, or how they are used. The default is
> FixUpX; choosing any other target is entirely your call.

### Your own domain

Know another FxEmbed-style mirror? In the **X / Twitter** submenu, click
**Add custom domain...** and type it (`myfx.com`, or a full URL; only the host
is kept). It is selected right away, and `x.com` / `twitter.com` links will be
rewritten to it. Use **Remove custom domain** to forget one again.

- The dialog is a PowerShell input box on Windows, and `kdialog` or `zenity`
  on Linux. If neither is installed, the domain list opens in your editor instead.
- Domains are stored one per line in `<config dir>/autofxembed/custom_domains.txt`
  (`~/.config/autofxembed/` on Linux). You can edit it by hand; lines starting
  with `#` are ignored, and changes show up the next time you open the menu.
- Domains that would be rewritten again (`x.com`, `twitter.com` and their
  subdomains) and the built-in hosts above are rejected.
- As with the built-ins, custom domains are third-party services. You are
  responsible for the ones you add.

## Install

Install the latest version straight from this repository with Cargo:

```bash
cargo install --git https://github.com/crisbbc/AutoFXEmbed.git --locked
```

Cargo puts the `autofxembed` executable in its bin directory, usually
`~/.cargo/bin` on Linux and `%USERPROFILE%\.cargo\bin` on Windows. Run it from
there, or make sure that directory is on your `PATH`.

## Usage

- **Windows:** double-click `autofxembed.exe`. No console window appears; a
  tray icon does.
- **Linux:** run `autofxembed`. A missing D-Bus StatusNotifier tray is treated
  as a startup failure, rather than leaving an unmanageable background process
  running.

Then copy an X / Twitter, Bluesky or TikTok link and paste it
anywhere. It is already embed-friendly.

## Tray menu

Right-click the tray icon for:

| Item                | What it does                                                      |
|---------------------|-------------------------------------------------------------------|
| **X / Twitter**     | Submenu to pick the target (FixUpX, BoyPussyX, MpregX, YaoiSex, FaggotX or one of your own). The choice is remembered. Also has **Add custom domain...** and **Remove custom domain**. |
| **TikTok**          | Informational: shows the host it is rewritten to.                 |
| **Recent**          | The last 10 converted links, labeled with their title and description. Click one to copy its embed link again. |
| **Clear history**   | Forgets every stored link.                                        |
| **Start on startup** | Checked when AutoFxEmbed will launch at login. On Windows this toggles a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; on Linux it creates `~/.config/autostart/autofxembed.desktop`. |
| **About**           | A small message box (a desktop notification on Linux).            |
| **Quit**            | Exits.                                                            |

## History

Every converted link is remembered: the last 100, and copying one again moves
it to the top. Once a link is converted, its embed page is fetched once in the
background to read the title and description, so the **Recent** submenu shows
what each link is.

- That fetch is the only network request AutoFxEmbed makes.
- If it fails, for example because the page has no embed data, the entry is
  dropped from the history.
- The history is stored in `<config dir>/autofxembed/history.json`
  (`~/.config/autofxembed/` on Linux).

## How changes are detected

Detection is event-driven wherever possible:

| Platform | Mechanism |
|----------|-----------|
| Windows  | The clipboard format listener. No polling, no CPU at idle. |
| X11      | XFixes selection-owner events, plus a slow 5 s safety read. |
| Wayland  | Data-control events (`ext-data-control` or `wlr-data-control`). The PRIMARY selection (middle-click paste) is rewritten too. On compositors without data-control, such as GNOME, it falls back to a 250 ms poll. |

## Build

```bash
cargo build --release
```

**Windows.** Requires the Rust toolchain with the MSVC linker
(`rustup default stable-x86_64-pc-windows-msvc`). The binary is
`target/release/autofxembed.exe`. Building embeds the FxEmbed icon as a
Windows resource (`build.rs` via `embed-resource`), which requires `rc.exe`
from the Windows SDK. It is included with Visual Studio Build Tools.

**Linux.** The binary is `target/release/autofxembed`. A desktop with a
StatusNotifierItem tray is required: KDE Plasma, or GNOME with an AppIndicator
extension.

## Development

```bash
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Unit tests cover the pure URL-rewrite logic; platform integration is tested
manually.
