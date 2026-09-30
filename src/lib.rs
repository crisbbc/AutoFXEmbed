pub mod autostart;
pub mod clipboard;
pub mod config;
pub mod monitor;
pub mod transform;
pub mod tray;
#[cfg(target_os = "linux")]
pub(crate) mod wayland_watch;
