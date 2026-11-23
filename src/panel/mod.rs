//! The tray flyout: a small borderless egui window that replaces the old
//! right-click menu. It runs in its own process (`autofxembed --panel`) so the
//! clipboard monitor's event loop never has to host a GUI loop.

mod ipc;
mod ui;

pub use ipc::{open_or_toggle, push_state, set_handlers};

use std::io::BufRead;
use std::sync::{mpsc, Arc, Mutex};

use ipc::PanelState;

const SIZE: [f32; 2] = [376.0, 620.0];

/// Entry point of the panel process.
pub fn run_panel() {
    let mut stdin = std::io::stdin().lock();
    let mut line = String::new();
    if stdin.read_line(&mut line).is_err() {
        return;
    }
    let state: PanelState = match serde_json::from_str(&line) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("AutoFxEmbed: bad panel state: {error}");
            return;
        }
    };
    drop(stdin);

    // Keep reading live `PanelState` updates pushed by the parent process.
    let (tx, rx) = mpsc::channel::<PanelState>();
    let egui_ctx: Arc<Mutex<Option<eframe::egui::Context>>> = Arc::new(Mutex::new(None));
    let reader_ctx = Arc::clone(&egui_ctx);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            if let Ok(updated) = serde_json::from_str::<PanelState>(&line) {
                if tx.send(updated).is_err() {
                    break;
                }
                if let Some(ctx) = reader_ctx.lock().ok().and_then(|g| g.clone()) {
                    ctx.request_repaint();
                }
            }
        }
    });

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
        Box::new(move |cc| {
            if let Ok(mut slot) = egui_ctx.lock() {
                *slot = Some(cc.egui_ctx.clone());
            }
            Ok(Box::new(ui::App::new(state, rx)))
        }),
    );
    if let Err(error) = result {
        eprintln!("AutoFxEmbed: panel failed: {error}");
    }
}
