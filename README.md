<p align="center">
  <img src="assets/fxembed.svg" alt="AutoFxEmbed logo" width="96" height="96">
</p>

<h1 align="center">AutoFxEmbed</h1>

<p align="center">
  A tiny tray utility that rewrites social media links on your clipboard so
  they embed properly in Discord, Telegram and friends.
</p>

<p align="center">
  <a href="https://github.com/crisbbc/AutoFXEmbed/actions/workflows/ci.yml"><img src="https://github.com/crisbbc/AutoFXEmbed/actions/workflows/ci.yml/badge.svg" alt="CI status"></a>
  <a href="https://github.com/crisbbc/AutoFXEmbed/releases/latest"><img src="https://img.shields.io/github/v/release/crisbbc/AutoFXEmbed" alt="Latest release"></a>
</p>

<p align="center">
  <a href="#install">Install</a> &middot;
  <a href="#usage">Usage</a> &middot;
  <a href="#tray-panel">Tray panel</a> &middot;
  <a href="#history">History</a> &middot;
  <a href="#updates">Updates</a> &middot;
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
icon's **X / Twitter** section; the choice is remembered.

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

Know another FxEmbed-style mirror? In the **X / Twitter** section, click
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

### Download a release

Grab a prebuilt binary from the
[latest release](https://github.com/crisbbc/AutoFXEmbed/releases/latest):

- **Windows:** `autofxembed-windows-x86_64.exe`
- **Linux:** `autofxembed-linux-x86_64` (run `chmod +x` on it first)

### Build with Cargo

Or install the latest source straight from this repository with Cargo:

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

## Tray panel

Left-click the tray icon to open a small flyout next to it. It closes when you click elsewhere or press Esc.

- **X / Twitter**: pick the target (FixUpX, BoyPussyX, MpregX, YaoiSex, FaggotX or one of your own). The choice is remembered. Type a domain and press **Add** to add your own, or click **✕** to remove one.
- **TikTok**: informational, shows the host it is rewritten to.
- **Recent**: the last 10 converted links, labeled with their title and description. Click one to copy its embed link again, or **Clear** to forget every stored link.
- **Start on startup**: checked when AutoFxEmbed will launch at login. On Windows this toggles a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; on Linux it creates `~/.config/autostart/autofxembed.desktop`.
- **Check for updates automatically** and **Check for updates now**: see [Updates](#updates).
- **Quit AutoFxEmbed**: exits.

Right-click the icon for a minimal **Open** / **Quit** menu, which is also the way in on Linux desktops whose tray never reports left-clicks. On Wayland the compositor decides where the panel appears, so it may not sit next to the icon. On Linux the panel needs the usual graphics libraries (OpenGL/EGL and xkbcommon).

## History

Every converted link is remembered: the last 100, and copying one again moves
it to the top. Once a link is converted, its embed page is fetched once in the
background to read the title and description, so the **Recent** list shows
what each link is.

- That fetch and the [update check](#updates) are the only network requests
  AutoFxEmbed makes.
- If it fails, for example because the page has no embed data, the entry is
  dropped from the history.
- The history is stored in `<config dir>/autofxembed/history.json`
  (`~/.config/autofxembed/` on Linux).

## Updates

AutoFxEmbed checks the [latest GitHub release](https://github.com/crisbbc/AutoFXEmbed/releases/latest)
shortly after it starts and then once a day. Turn that off with **Check for
updates automatically** in the tray panel, or check on demand with **Check for
updates...**.

- When a newer version exists it asks first. On **Yes** it downloads the
  release binary for your platform, verifies its SHA-256 against the digest
  GitHub publishes for the asset, replaces the running executable and
  restarts. A failed check or download changes nothing.
- Declining an automatic prompt skips that version; the manual check still
  offers it.
- If you installed with `cargo install`, the binary isn't swapped under Cargo:
  you are told to re-run the install command instead.
- Linux needs `kdialog` or `zenity` for the confirmation prompt.
- State is stored in `<config dir>/autofxembed/updates.json`
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
