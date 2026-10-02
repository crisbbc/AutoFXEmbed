//! egui layout of the tray flyout.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText,
    Sense, Shadow, Stroke, StrokeKind, TextureHandle, Vec2,
};

use super::ipc::{Action, PanelState};
use super::SIZE;
use crate::config;

const TOAST: Duration = Duration::from_millis(1500);
const EDGE_GAP: f32 = 8.0;
/// Room around the card for its drop shadow (the window itself is transparent).
const SHADOW_MARGIN: i8 = 10;
const ROW_HEIGHT: f32 = 34.0;

/// Colors of the panel, per light / dark theme.
#[derive(Clone, Copy)]
struct Palette {
    window: Color32,
    card: Color32,
    hover: Color32,
    border: Color32,
    text: Color32,
    weak: Color32,
    accent: Color32,
    on_accent: Color32,
    danger: Color32,
    success: Color32,
}

impl Palette {
    const DARK: Self = Self {
        window: Color32::from_rgb(0x15, 0x17, 0x1c),
        card: Color32::from_rgb(0x1e, 0x21, 0x29),
        hover: Color32::from_rgb(0x28, 0x2c, 0x37),
        border: Color32::from_rgb(0x2c, 0x31, 0x3c),
        text: Color32::from_rgb(0xe7, 0xe9, 0xee),
        weak: Color32::from_rgb(0x8b, 0x93, 0xa3),
        accent: Color32::from_rgb(0x4f, 0x8e, 0xff),
        on_accent: Color32::WHITE,
        danger: Color32::from_rgb(0xf0, 0x6a, 0x6a),
        success: Color32::from_rgb(0x4c, 0xc3, 0x8a),
    };

    const LIGHT: Self = Self {
        window: Color32::from_rgb(0xf4, 0xf5, 0xf8),
        card: Color32::WHITE,
        hover: Color32::from_rgb(0xee, 0xf0, 0xf4),
        border: Color32::from_rgb(0xe1, 0xe4, 0xea),
        text: Color32::from_rgb(0x1b, 0x1f, 0x26),
        weak: Color32::from_rgb(0x6b, 0x72, 0x80),
        accent: Color32::from_rgb(0x2f, 0x6f, 0xeb),
        on_accent: Color32::WHITE,
        danger: Color32::from_rgb(0xd9, 0x48, 0x48),
        success: Color32::from_rgb(0x1f, 0x9d, 0x63),
    };

    fn for_theme(theme: egui::Theme) -> Self {
        match theme {
            egui::Theme::Dark => Self::DARK,
            egui::Theme::Light => Self::LIGHT,
        }
    }
}

pub struct App {
    state: PanelState,
    new_domain: String,
    error: Option<String>,
    show_about: bool,
    copied: Option<Instant>,
    checking: Option<Instant>,
    seen_focus: bool,
    positioned: bool,
    icon: Option<TextureHandle>,
}

impl App {
    pub fn new(state: PanelState) -> Self {
        Self {
            state,
            new_domain: String::new(),
            error: None,
            show_about: false,
            copied: None,
            checking: None,
            seen_focus: false,
            positioned: false,
            icon: None,
        }
    }

    fn send(&self, action: &Action) {
        use std::io::Write;
        if let Ok(line) = serde_json::to_string(action) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }

    /// Put the window next to the tray icon, kept inside the monitor. The
    /// anchor arrives in screen pixels, egui positions in points.
    fn place(&mut self, ctx: &egui::Context) {
        if self.positioned {
            return;
        }
        let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) else {
            return;
        };
        self.positioned = true;
        let Some((ax, ay)) = self.state.anchor else {
            return;
        };
        let scale = ctx.pixels_per_point();
        let (ax, ay) = (ax / scale, ay / scale);
        let x = (ax - SIZE[0] / 2.0).clamp(EDGE_GAP, (monitor.x - SIZE[0] - EDGE_GAP).max(EDGE_GAP));
        // Taskbars sit at the bottom or top: open away from the nearer edge.
        let y = if ay > monitor.y / 2.0 {
            ay - SIZE[1] - EDGE_GAP
        } else {
            ay + EDGE_GAP
        };
        let y = y.clamp(EDGE_GAP, (monitor.y - SIZE[1] - EDGE_GAP).max(EDGE_GAP));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
    }

    /// The tray icon, decoded from the ARGB pixels build.rs prepared for ksni.
    fn icon(&mut self, ctx: &egui::Context) -> Option<&TextureHandle> {
        if self.icon.is_none() {
            let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/icon.argb"));
            let w = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
            let h = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
            let argb = bytes.get(8..)?;
            if w * h * 4 != argb.len() {
                return None;
            }
            let rgba: Vec<u8> = argb
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[1], p[2], p[3], p[0]])
                .collect();
            let image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
            self.icon = Some(ctx.load_texture("app-icon", image, egui::TextureOptions::LINEAR));
        }
        self.icon.as_ref()
    }

    fn add_domain(&mut self) {
        match config::normalize_domain(&self.new_domain) {
            Ok(domain) => {
                if config::is_builtin_domain(&domain) {
                    self.error = Some(format!("{domain} is already built in"));
                    return;
                }
                self.send(&Action::AddCustom(domain.clone()));
                if !self.state.customs.contains(&domain) {
                    self.state.customs.push(domain.clone());
                }
                self.state.selected_builtin = None;
                self.state.selected_custom = Some(domain);
                self.new_domain.clear();
                self.error = None;
            }
            Err(reason) => self.error = Some(reason.to_string()),
        }
    }

    fn select_builtin(&mut self, id: &str) {
        self.send(&Action::SelectBuiltin(id.to_string()));
        self.state.selected_builtin = Some(id.to_string());
        self.state.selected_custom = None;
    }

    fn select_custom(&mut self, domain: &str) {
        self.send(&Action::SelectCustom(domain.to_string()));
        self.state.selected_builtin = None;
        self.state.selected_custom = Some(domain.to_string());
    }

    fn remove_custom(&mut self, domain: &str) {
        self.send(&Action::RemoveCustom(domain.to_string()));
        self.state.customs.retain(|d| d != domain);
        if self.state.selected_custom.as_deref() == Some(domain) {
            self.state.selected_custom = None;
            self.state.selected_builtin = Some(config::XTarget::FixUp.id().to_string());
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, pal: Palette) {
        ui.horizontal(|ui| {
            if let Some(icon) = self.icon(ui.ctx()) {
                ui.add(egui::Image::new((icon.id(), Vec2::splat(28.0))));
            }
            ui.add_space(2.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("AutoFxEmbed").size(16.0).strong().color(pal.text));
                ui.label(
                    RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .size(11.0)
                        .color(pal.weak),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Info, pal, self.show_about)
                    .on_hover_text("About")
                    .clicked()
                {
                    self.show_about = !self.show_about;
                }
            });
        });
        if self.show_about {
            ui.add_space(6.0);
            card(ui, pal, |ui| {
                ui.label(RichText::new(&self.state.about).color(pal.weak));
            });
        }
    }

    fn targets(&mut self, ui: &mut egui::Ui, pal: Palette) {
        section_title(ui, pal, "X / Twitter links go to");
        card(ui, pal, |ui| {
            let targets = self.state.targets.clone();
            for (id, label) in &targets {
                let selected = self.state.selected_builtin.as_deref() == Some(id);
                let (name, host) = split_label(label);
                if choice_row(ui, pal, selected, name, host, false).clicked && !selected {
                    self.select_builtin(id);
                }
            }
            for domain in self.state.customs.clone() {
                let selected = self.state.selected_custom.as_deref() == Some(domain.as_str());
                let row = choice_row(ui, pal, selected, &domain, "custom", true);
                if row.removed {
                    self.remove_custom(&domain);
                } else if row.clicked && !selected {
                    self.select_custom(&domain);
                }
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.new_domain)
                        .hint_text(RichText::new("Add your own domain…").color(pal.weak))
                        .margin(Margin::symmetric(8, 6))
                        .background_color(pal.window)
                        .desired_width(ui.available_width() - 58.0),
                );
                let submitted = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let enabled = !self.new_domain.trim().is_empty();
                if (accent_button(ui, pal, "Add", enabled).clicked() || submitted) && enabled {
                    self.add_domain();
                }
            });
            if let Some(error) = &self.error {
                ui.add_space(2.0);
                ui.label(RichText::new(error).size(11.5).color(pal.danger));
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new("TikTok links always use tnktok.com")
                .size(11.5)
                .color(pal.weak),
        );
    }

    fn recent(&mut self, ui: &mut egui::Ui, pal: Palette) {
        ui.horizontal(|ui| {
            section_title(ui, pal, "Recent");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.copied.is_some_and(|at| at.elapsed() < TOAST) {
                    pill(ui, "Copied", pal.success);
                } else if !self.state.recent.is_empty() && link_button(ui, pal, "Clear").clicked() {
                    self.send(&Action::ClearHistory);
                    self.state.recent.clear();
                }
            });
        });
        let mut copy = None;
        card(ui, pal, |ui| {
            if self.state.recent.is_empty() {
                ui.add_sized(
                    [ui.available_width(), 44.0],
                    egui::Label::new(
                        RichText::new("Converted links will show up here").color(pal.weak),
                    ),
                );
                return;
            }
            for entry in &self.state.recent {
                let title = entry
                    .title
                    .as_deref()
                    .or(entry.site.as_deref())
                    .unwrap_or(&entry.embed);
                let site = entry.site.as_deref().unwrap_or("");
                if recent_row(ui, pal, site, title, &entry.embed).clicked() {
                    copy = Some(entry.embed.clone());
                }
            }
        });
        if let Some(embed) = copy {
            self.send(&Action::CopyRecent(embed));
            self.copied = Some(Instant::now());
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui, pal: Palette) {
        section_title(ui, pal, "Settings");
        card(ui, pal, |ui| {
            if switch_row(ui, pal, "Start on startup", &mut self.state.startup) {
                self.send(&Action::ToggleStartup);
            }
            divider(ui, pal);
            if switch_row(ui, pal, "Check for updates automatically", &mut self.state.auto_update) {
                self.send(&Action::ToggleAutoUpdate);
            }
            divider(ui, pal);
            let checking = self.checking.is_some_and(|at| at.elapsed() < TOAST * 2);
            let label = if checking { "Checking for updates…" } else { "Check for updates now" };
            if action_row(ui, pal, label, pal.accent, !checking).clicked() {
                self.send(&Action::CheckUpdate);
                self.checking = Some(Instant::now());
            }
        });
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.place(ctx);

        // Behave like a flyout: close on Esc or when focus moves elsewhere.
        match ctx.input(|i| i.viewport().focused) {
            Some(true) => self.seen_focus = true,
            Some(false) if self.seen_focus => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Repaint so the toast labels expire without further input.
        ctx.request_repaint_after(Duration::from_millis(500));

        let pal = Palette::for_theme(ctx.theme());
        apply_style(ctx, pal);

        let frame = egui::Frame::new()
            .fill(pal.window)
            .stroke(Stroke::new(1.0_f32, pal.border))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::same(14))
            .outer_margin(Margin::same(SHADOW_MARGIN))
            .shadow(Shadow {
                offset: [0, 4],
                blur: 14,
                spread: 0,
                color: Color32::from_black_alpha(90),
            });

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            self.header(ui, pal);
            ui.add_space(10.0);
            let body = ui.available_height() - 46.0;
            egui::ScrollArea::vertical()
                .max_height(body)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    self.targets(ui, pal);
                    ui.add_space(12.0);
                    self.recent(ui, pal);
                    ui.add_space(12.0);
                    self.settings(ui, pal);
                    ui.add_space(4.0);
                });
            ui.add_space(8.0);
            if quit_button(ui, pal).clicked() {
                self.send(&Action::Quit);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}

/// Map the palette onto egui's built-in widgets (text fields, scroll bars,
/// tooltips) so they match the hand-drawn rows.
fn apply_style(ctx: &egui::Context, pal: Palette) {
    ctx.style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 4.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.scroll = egui::style::ScrollStyle::thin();
        style.interaction.selectable_labels = false;

        let v = &mut style.visuals;
        v.dark_mode = pal.window == Palette::DARK.window;
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = pal.window;
        v.extreme_bg_color = pal.window;
        v.override_text_color = Some(pal.text);
        v.hyperlink_color = pal.accent;
        v.selection.bg_fill = pal.accent.gamma_multiply(0.35);
        v.selection.stroke = Stroke::new(1.0_f32, pal.accent);
        v.text_cursor.stroke = Stroke::new(2.0_f32, pal.accent);
        v.window_corner_radius = CornerRadius::same(10);
        v.window_stroke = Stroke::new(1.0_f32, pal.border);
        v.popup_shadow = Shadow::NONE;

        let radius = CornerRadius::same(8);
        for (w, fill, stroke) in [
            (&mut v.widgets.noninteractive, pal.card, pal.border),
            (&mut v.widgets.inactive, pal.window, pal.border),
            (&mut v.widgets.hovered, pal.hover, pal.weak),
            (&mut v.widgets.active, pal.hover, pal.accent),
            (&mut v.widgets.open, pal.hover, pal.accent),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0_f32, stroke);
            w.corner_radius = radius;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal.text);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, pal.text);
        v.widgets.active.fg_stroke = Stroke::new(1.0_f32, pal.text);
    });
}

fn section_title(ui: &mut egui::Ui, pal: Palette, title: &str) {
    ui.add_space(2.0);
    ui.label(
        RichText::new(title.to_uppercase())
            .size(10.5)
            .strong()
            .extra_letter_spacing(0.8)
            .color(pal.weak),
    );
    ui.add_space(2.0);
}

/// A rounded card holding a group of rows.
fn card(ui: &mut egui::Ui, pal: Palette, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(pal.card)
        .stroke(Stroke::new(1.0_f32, pal.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            add_contents(ui);
        });
}

fn divider(ui: &mut egui::Ui, pal: Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        rect.x_range().shrink(8.0),
        rect.center().y,
        Stroke::new(1.0_f32, pal.border),
    );
}

/// Hover / press background shared by every clickable row.
fn row_background(ui: &egui::Ui, pal: Palette, rect: Rect, response: &egui::Response) {
    let t = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    if t > 0.0 {
        let fill = if response.is_pointer_button_down_on() {
            pal.hover.gamma_multiply(1.4)
        } else {
            pal.hover.gamma_multiply(t)
        };
        ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    }
}

/// Text clipped to `max_width` with a trailing ellipsis.
fn painted_text(
    ui: &egui::Ui,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Rect {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    let galley = ui.painter().layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    ui.painter().galley(rect.min, galley, color);
    rect
}

struct RowResponse {
    clicked: bool,
    removed: bool,
}

/// A radio-style row: indicator, name, and the host in muted text on the right.
/// Custom rows show a remove button in place of the host while hovered.
fn choice_row(
    ui: &mut egui::Ui,
    pal: Palette,
    selected: bool,
    name: &str,
    host: &str,
    removable: bool,
) -> RowResponse {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    row_background(ui, pal, rect, &response);

    let t = ui.ctx().animate_bool_with_time(response.id.with("sel"), selected, 0.15);
    let dot = Pos2::new(rect.left() + 16.0, rect.center().y);
    let ring = pal.weak.lerp_to_gamma(pal.accent, t);
    ui.painter().circle_stroke(dot, 7.0, Stroke::new(1.5_f32, ring));
    if t > 0.0 {
        ui.painter().circle_filled(dot, 4.0 * t, pal.accent);
    }

    let mut removed = false;
    let right_reserved = if removable {
        let btn = Rect::from_center_size(Pos2::new(rect.right() - 18.0, rect.center().y), Vec2::splat(22.0));
        let hovered_row = response.hovered() || ui.rect_contains_pointer(btn);
        if hovered_row {
            let btn_response = ui.interact(btn, response.id.with("remove"), Sense::click());
            let color = if btn_response.hovered() { pal.danger } else { pal.weak };
            if btn_response.hovered() {
                ui.painter().rect_filled(btn, CornerRadius::same(6), pal.danger.gamma_multiply(0.15));
            }
            draw_cross(ui, btn.center(), 4.0, Stroke::new(1.5_f32, color));
            removed = btn_response.on_hover_text("Remove domain").clicked();
        } else {
            painted_text(
                ui,
                Pos2::new(rect.right() - 10.0, rect.center().y),
                Align2::RIGHT_CENTER,
                host,
                FontId::proportional(11.5),
                pal.weak,
                80.0,
            );
        }
        40.0
    } else {
        let host_rect = painted_text(
            ui,
            Pos2::new(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            host,
            FontId::proportional(11.5),
            pal.weak,
            rect.width() * 0.45,
        );
        host_rect.width() + 18.0
    };

    let name_color = if selected { pal.text } else { pal.text.gamma_multiply(0.85) };
    painted_text(
        ui,
        Pos2::new(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.5),
        name_color,
        rect.width() - 32.0 - right_reserved,
    );

    let clicked = response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !removed;
    RowResponse { clicked, removed }
}

/// A history entry: a site monogram, the title, and the embed link.
fn recent_row(ui: &mut egui::Ui, pal: Palette, site: &str, title: &str, embed: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
    row_background(ui, pal, rect, &response);

    let badge = Rect::from_center_size(Pos2::new(rect.left() + 20.0, rect.center().y), Vec2::splat(26.0));
    ui.painter().rect_filled(badge, CornerRadius::same(7), pal.accent.gamma_multiply(0.18));
    let initial = site
        .chars()
        .chain(title.chars())
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "#".into());
    ui.painter().text(
        badge.center(),
        Align2::CENTER_CENTER,
        initial,
        FontId::proportional(13.0),
        pal.accent,
    );

    let text_left = rect.left() + 42.0;
    let width = rect.right() - text_left - 10.0;
    painted_text(
        ui,
        Pos2::new(text_left, rect.top() + 6.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(13.0),
        pal.text,
        width,
    );
    let link = embed.trim_start_matches("https://").trim_start_matches("http://");
    painted_text(
        ui,
        Pos2::new(text_left, rect.top() + 23.0),
        Align2::LEFT_TOP,
        link,
        FontId::proportional(11.0),
        pal.weak,
        width,
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Click to copy")
}

/// A label with a toggle switch on the right. Returns true when it flipped.
fn switch_row(ui: &mut egui::Ui, pal: Palette, label: &str, on: &mut bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    row_background(ui, pal, rect, &response);
    let flipped = response.clicked();
    if flipped {
        *on = !*on;
    }

    painted_text(
        ui,
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        pal.text,
        rect.width() - 64.0,
    );

    let track = Rect::from_center_size(Pos2::new(rect.right() - 28.0, rect.center().y), Vec2::new(34.0, 20.0));
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.15);
    let radius = track.height() / 2.0;
    ui.painter().rect_filled(track, radius, pal.border.lerp_to_gamma(pal.accent, t));
    let knob_x = track.left() + radius + (track.width() - 2.0 * radius) * t;
    ui.painter().circle_filled(Pos2::new(knob_x, track.center().y), radius - 3.0, Color32::WHITE);
    response.on_hover_cursor(egui::CursorIcon::PointingHand);
    flipped
}

/// A full-width row that triggers something, with accent-colored text.
fn action_row(ui: &mut egui::Ui, pal: Palette, label: &str, color: Color32, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), sense);
    if enabled {
        row_background(ui, pal, rect, &response);
    }
    painted_text(
        ui,
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        if enabled { color } else { pal.weak },
        rect.width() - 20.0,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn accent_button(ui: &mut egui::Ui, pal: Palette, label: &str, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(50.0, 30.0), Sense::click());
    let fill = if !enabled {
        pal.accent.gamma_multiply(0.35)
    } else if response.is_pointer_button_down_on() {
        pal.accent.gamma_multiply(0.8)
    } else if response.hovered() {
        pal.accent.gamma_multiply(1.15)
    } else {
        pal.accent
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.0),
        pal.on_accent,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn link_button(ui: &mut egui::Ui, pal: Palette, label: &str) -> egui::Response {
    let response = ui.add(
        egui::Label::new(RichText::new(label).size(11.5).color(pal.accent)).sense(Sense::click()),
    );
    if response.hovered() {
        let r = response.rect;
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0_f32, pal.accent));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(8, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color));
        });
}

fn quit_button(ui: &mut egui::Ui, pal: Palette) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);
    let fill = pal.card.lerp_to_gamma(pal.danger.gamma_multiply(0.18), t);
    ui.painter().rect(
        rect,
        CornerRadius::same(9),
        fill,
        Stroke::new(1.0_f32, pal.border.lerp_to_gamma(pal.danger.gamma_multiply(0.6), t)),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Quit AutoFxEmbed",
        FontId::proportional(13.5),
        pal.text.lerp_to_gamma(pal.danger, t),
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

enum Icon {
    Info,
}

/// A small icon button. Drawn with the painter because the default egui
/// fonts lack glyphs such as ✕ and ⓘ.
fn icon_button(ui: &mut egui::Ui, icon: Icon, pal: Palette, active: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if response.hovered() || active {
        ui.painter().rect_filled(rect, CornerRadius::same(8), pal.hover);
    }
    let color = if active { pal.accent } else { pal.weak };
    let stroke = Stroke::new(1.5_f32, color);
    let c = rect.center();
    match icon {
        Icon::Info => {
            ui.painter().circle_stroke(c, 8.0, stroke);
            ui.painter()
                .line_segment([c + Vec2::new(0.0, -0.5), c + Vec2::new(0.0, 4.0)], stroke);
            ui.painter().circle_filled(c + Vec2::new(0.0, -3.6), 1.1, color);
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn draw_cross(ui: &egui::Ui, c: Pos2, d: f32, stroke: Stroke) {
    ui.painter().line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], stroke);
    ui.painter().line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], stroke);
}

/// `"FixUpX (fxtwitter.com / fixupx.com)"` → `("FixUpX", "fxtwitter.com / fixupx.com")`.
fn split_label(label: &str) -> (&str, &str) {
    match label.split_once(" (") {
        Some((name, rest)) => (name, rest.trim_end_matches(')')),
        None => (label, ""),
    }
}
