// Hide the console window on Windows; ignored on other platforms.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    autofxembed::monitor::run();
}
