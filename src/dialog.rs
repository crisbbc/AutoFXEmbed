//! Small desktop dialogs used by the tray: a one-line text prompt and a
//! notification, built on tools the OS already ships (PowerShell on Windows,
//! `kdialog` / `zenity` / `notify-send` on Linux), so no GUI toolkit is linked.

#[cfg(target_os = "linux")]
use std::process::Command;

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

/// Blocking yes/no question. Returns false on "no", on close, or when no
/// dialog tool exists (in which case the message is shown as a notification).
#[cfg(target_os = "linux")]
pub fn confirm(title: &str, message: &str) -> bool {
    let attempts: [(&str, Vec<&str>); 2] = [
        ("kdialog", vec!["--title", title, "--yesno", message]),
        (
            "zenity",
            vec!["--question", "--title", title, "--text", message],
        ),
    ];
    for (program, args) in attempts {
        match Command::new(program).args(&args).status() {
            Ok(status) => return status.success(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                eprintln!("AutoFxEmbed: unable to run {program}: {error}");
                continue;
            }
        }
    }
    notify(
        title,
        &format!("{message}\n(Install kdialog or zenity to update from the tray, or download it manually.)"),
    );
    false
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

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// Blocking yes/no question.
#[cfg(target_os = "windows")]
pub fn confirm(title: &str, message: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONQUESTION, MB_SETFOREGROUND, MB_YESNO,
    };
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            crate::clipboard::wide(message).as_ptr(),
            crate::clipboard::wide(title).as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND,
        ) == IDYES
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
