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
  <a href="#android">Android</a> &middot;
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

- Everything else in the URL is preserved: port, path, query and fragment,
  minus [tracking parameters](#link-trackers). Content subdomains like
  `mobile.`/`www.` are normalized cleanly (while TikTok short-link subdomains
  `vm.`/`vt.` are preserved). Non-content subdomains (such as `developer.x.com`
  or `docs.bsky.app`) and non-post pages (like `/settings`, `/home`, or bare
  root URLs) are left untouched.
- Each built-in site can be toggled on or off individually, and you can add
  custom `source -> target` site rules (for example `reddit.com -> rxddit.com`
  or `instagram.com -> kkinstagram.com`) right from the panel or
  `<config dir>/autofxembed/extra_rules.txt`.
- Optional **Auto-translate** appends an FxEmbed language code (e.g. `en`) to X
  status URLs.
- Links embedded in prose are rewritten too, including ones wrapped in
  brackets or quotes, like `(https://x.com/...)`.
- Already-converted links and unrelated URLs are left untouched.

### Link trackers

Tracking parameters are removed from copied links, both from the links above
and from any other `http(s)` link, so `https://example.com/page?utm_source=x`
becomes `https://example.com/page`. Turn it off with **Remove link trackers**
in the tray panel.

- Removed everywhere: `utm_*`, `fbclid`, `gclid`, `gclsrc`, `dclid`, `gbraid`,
  `wbraid`, `msclkid`, `twclid`, `ttclid`, `yclid`, `mc_cid`, `mc_eid`,
  `_hsenc`, `_hsmi`, `mkt_tok`, `igshid`, `igsh`.
- X / Twitter post links lose their whole query (`?s=`, `?t=`, `?a=`, ...),
  also when they are already on an embed host such as `fixupx.com`. Other X
  links lose `s`, `t`, `a`, `ref_src` and `ref_url`.
- Removed only on their own site: TikTok's share parameters such as
  `is_from_webapp` and `sender_device`; `si`, `pp`, `feature` (YouTube); `si`, `nd` (Spotify);
  `share_id`, `rdt`, `ref_source` (Reddit).
- Links that were only cleaned are not added to the [history](#history).
- The setting is stored in `<config dir>/autofxembed/strip_trackers`.

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

Know another FxEmbed-style mirror? In the **X / Twitter** section of the tray
panel (or Android settings), type it (`myfx.com`, or a full URL; only the host
is kept) and press **Add**. It is selected right away, and `x.com` /
`twitter.com` links will be rewritten to it. Click **✕** to remove one again.

- Domains are stored one per line in `<config dir>/autofxembed/custom_domains.txt`
  (`~/.config/autofxembed/` on Linux). You can edit it by hand; lines starting
  with `#` are ignored, and changes show up the next time you open the panel.
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

Left-click the tray icon to open a small flyout next to it. It updates live while open and closes when you click elsewhere or press Esc.

- **Pause / Resume**: temporarily pause clipboard rewriting without quitting (also available in the right-click menu; the tray icon dims on Linux while paused).
- **Sites**: toggle **X / Twitter**, **Bluesky**, and **TikTok** individually, or add custom `source -> target` site rules.
- **X / Twitter**: pick the target (FixUpX, BoyPussyX, MpregX, YaoiSex, FaggotX or one of your own), add/remove custom domains, and optionally configure an **Auto-translate** language code (e.g. `en`).
- **Recent**: converted links with their title, description snippet, and relative age, plus live search and **Show all** (up to 100 entries). Click one to copy its embed link, hover for **RAW** (copy the original link without re-rewriting) or **✕** (remove), or **Clear** to forget every stored link.
- **Remove link trackers**: see [Link trackers](#link-trackers).
- **Start on startup**: checked when AutoFxEmbed will launch at login. On Windows this toggles a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; on Linux it creates `~/.config/autostart/autofxembed.desktop`.
- **Check for updates automatically** and **Check for updates now**: see [Updates](#updates).
- **Quit AutoFxEmbed**: exits.

Right-click the icon for a minimal **Open** / **Pause** / **Quit** menu, which is also the way in on Linux desktops whose tray never reports left-clicks. On Wayland the compositor decides where the panel appears, so it may not sit next to the icon. On Linux the panel needs the usual graphics libraries (OpenGL/EGL and xkbcommon).

## History

Every converted link is remembered: the last 100, and copying one again moves
it to the top. Once a link is converted, its embed page is fetched once in the
background to read the title and description, so the **Recent** list shows
what each link is.

- That fetch and the [update check](#updates) are the only network requests
  AutoFxEmbed makes.
- Confirmed non-embed pages (such as a `4xx` or an unrelated redirect) are
  dropped; transient offline or `5xx`/`429` errors keep the entry and retry
  automatically the next time you open the panel or restart.
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

## Android

Android doesn't let background apps watch the clipboard, so on Android you
share links to the app—or tap **Convert clipboard** inside the app. Share a
link from X, Bluesky, TikTok, a browser or any other app and pick either:

- **Copy embed link**: converts the text and copies it to your clipboard.
- **Share embed link**: converts the text and immediately opens the Android
  share sheet so you can forward it straight to Discord, Telegram, etc.

The app's screen has the same settings as the desktop panel:

- **Sites**: toggle X / Twitter, Bluesky, and TikTok individually, or add
  custom `source -> target` site rules.
- **X / Twitter**: pick a built-in target, add/remove your own domains, and
  optionally set an **Auto-translate** language code.
- **Remove link trackers**: see [Link trackers](#link-trackers).
- **Recent**: the last 10 converted links with their title and description.
  Tap one to copy it again, long-press to copy the original link, share, or
  remove, or tap **Clear** to forget them. History is excluded from Android
  cloud backups. The embed page is read once, as on the desktop: confirmed
  non-embeds are dropped, while offline or `5xx`/`429` errors retry the next
  time you open the app.
- **Updates**: checked against the latest GitHub release when you open the
  app, at most once a day, or with **Check for updates now**. The APK is
  verified against GitHub's SHA-256 digest before Android's installer asks
  you to confirm. The first time, Android also asks you to allow installs from
  AutoFxEmbed.

Grab `autofxembed-android.apk` from the
[latest release](https://github.com/crisbbc/AutoFXEmbed/releases/latest).
Minimum Android version: 7.0. It uses the same code as the desktop app for
rewriting, domain checks, embed data and version comparison (the `core`
crate, called through JNI from `android/rust`).

### Building the APK

You need the Android SDK with NDK `30.0.16248370`, JDK 17,
[`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk) and the Android Rust targets:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk --locked
cd android
./gradlew assembleDebug   # app/build/outputs/apk/debug/app-debug.apk
```

Gradle runs `cargo ndk` itself. Point it at the SDK with `ANDROID_HOME` or
`android/local.properties` (`sdk.dir=...`). The app version is read from the
root `Cargo.toml`.

### Signing releases

Android only installs an update signed with the same key as the installed
app, so release APKs are signed with one long-lived key. Create it once and
keep a backup: if it is lost, users have to uninstall and reinstall.

```bash
keytool -genkeypair -v -keystore autofxembed-release.jks -alias autofxembed \
  -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 autofxembed-release.jks   # value of ANDROID_KEYSTORE_BASE64
```

Add these repository secrets: `ANDROID_KEYSTORE_BASE64`,
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` (`autofxembed`) and
`ANDROID_KEY_PASSWORD`. Without them the release workflow skips the APK. Debug
builds use a different key, so switching between a debug build and a release
build needs an uninstall.

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
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Unit tests cover the pure URL-rewrite logic (in `core/`); platform
integration is tested manually.
