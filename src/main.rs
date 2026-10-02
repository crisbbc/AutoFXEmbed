// Hide the console window on Windows; ignored on other platforms.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    // The tray flyout runs as a child process of the monitor (see `panel`).
    if std::env::args().nth(1).as_deref() == Some("--panel") {
        autofxembed::panel::run_panel();
    } else {
        autofxembed::monitor::run();
    }
}
