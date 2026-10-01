//! Small desktop dialogs used by the tray: a one-line text prompt and a
//! notification, built on tools the OS already ships (PowerShell on Windows,
//! `kdialog` / `zenity` / `notify-send` on Linux), so no GUI toolkit is linked.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// Outcome of [`prompt`].
#[derive(Debug, PartialEq, Eq)]
enum Prompt {
    Text(String),
    Cancelled,
    /// No dialog tool could be started.
    Unavailable,
}

/// True while the add-domain dialog is open, so repeated clicks don't stack them.
static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

/// Ask the user for a custom embed domain and add it. Returns immediately: the
/// dialog runs on its own thread so the tray / message loop is never blocked.
pub fn add_custom_domain() {
    if DIALOG_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        add_custom_domain_blocking();
        DIALOG_OPEN.store(false, Ordering::SeqCst);
    });
}

fn add_custom_domain_blocking() {
    let message = "Embed domain for X / Twitter links (for example myfx.com):";
    match prompt("Add custom domain", message) {
        Prompt::Text(input) => match crate::config::add_custom(&input) {
            Ok(domain) => notify(
                "AutoFxEmbed",
                &format!("X / Twitter links now go to {domain}"),
            ),
            Err(reason) => notify("AutoFxEmbed: domain not added", &reason),
        },
        Prompt::Cancelled => {}
        Prompt::Unavailable => edit_domain_file(),
    }
}

/// Fallback when no dialog tool exists: open the domain list in the default editor.
fn edit_domain_file() {
    let Some(path) = crate::config::custom_domains_path() else {
        return;
    };
    if !path.exists() {
        let created = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, "# One embed domain per line\n"));
        if let Err(error) = created {
            eprintln!("AutoFxEmbed: unable to create {path:?}: {error}");
            return;
        }
    }
    match open_file(&path) {
        Ok(()) => notify(
            "AutoFxEmbed",
            "Add one domain per line, save, then reopen the tray menu.",
        ),
        Err(error) => {
            eprintln!("AutoFxEmbed: unable to open {path:?}: {error}");
            notify(
                "AutoFxEmbed",
                &format!("Add domains to {} (one per line).", path.display()),
            );
        }
    }
}

/// Run `command` to completion without leaving a zombie behind.
#[cfg(target_os = "linux")]
fn spawn_detached(command: &mut Command) -> std::io::Result<()> {
    let mut child = command.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn prompt(title: &str, message: &str) -> Prompt {
    let attempts: [(&str, Vec<&str>); 2] = [
        ("kdialog", vec!["--title", title, "--inputbox", message]),
        (
            "zenity",
            vec!["--entry", "--title", title, "--text", message],
        ),
    ];
    for (program, args) in attempts {
        match Command::new(program).args(&args).output() {
            Ok(output) if output.status.success() => {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return if text.is_empty() {
                    Prompt::Cancelled
                } else {
                    Prompt::Text(text)
                };
            }
            Ok(_) => return Prompt::Cancelled,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                eprintln!("AutoFxEmbed: unable to run {program}: {error}");
                continue;
            }
        }
    }
    Prompt::Unavailable
}

/// Best-effort desktop notification (failures are only logged).
#[cfg(target_os = "linux")]
pub fn notify(title: &str, body: &str) {
    let result = spawn_detached(
        Command::new("notify-send")
            .arg("--app-name=AutoFxEmbed")
            .arg(title)
            .arg(body),
    );
    if let Err(error) = result {
        eprintln!("AutoFxEmbed: unable to show notification: {error}");
    }
}

#[cfg(target_os = "linux")]
fn open_file(path: &std::path::Path) -> std::io::Result<()> {
    spawn_detached(Command::new("xdg-open").arg(path))
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn prompt(title: &str, message: &str) -> Prompt {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Single-quoted PowerShell strings: only `'` needs doubling.
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; \
         [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; \
         [Microsoft.VisualBasic.Interaction]::InputBox({}, {})",
        quote(message),
        quote(title),
    );
    match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // InputBox returns an empty string when the user cancels.
            if text.is_empty() {
                Prompt::Cancelled
            } else {
                Prompt::Text(text)
            }
        }
        Ok(_) => Prompt::Cancelled,
        Err(error) => {
            eprintln!("AutoFxEmbed: unable to run powershell: {error}");
            Prompt::Unavailable
        }
    }
}

#[cfg(target_os = "windows")]
pub fn notify(title: &str, body: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            crate::clipboard::wide(body).as_ptr(),
            crate::clipboard::wide(title).as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(target_os = "windows")]
fn open_file(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("notepad")
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}
