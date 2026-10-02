//! The tray flyout: a small borderless egui window that replaces the old
//! right-click menu. It runs in its own process (`autofxembed --panel`) so the
//! clipboard monitor's event loop never has to host a GUI loop.

mod ipc;
mod ui;

pub use ipc::{open_or_toggle, set_handlers};

use std::io::BufRead;

use ipc::PanelState;

const SIZE: [f32; 2] = [360.0, 580.0];

/// Entry point of the panel process.
pub fn run_panel() {
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return;
    }
    let state: PanelState = match serde_json::from_str(&line) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("AutoFxEmbed: bad panel state: {error}");
            return;
        }
    };

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size(SIZE)
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_transparent(true);
    if let Some((x, y)) = state.anchor {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let result = eframe::run_native(
        "AutoFxEmbed",
        options,
        Box::new(|_cc| Ok(Box::new(ui::App::new(state)))),
    );
    if let Err(error) = result {
        eprintln!("AutoFxEmbed: panel failed: {error}");
    }
}
