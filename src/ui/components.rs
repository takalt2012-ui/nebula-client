use eframe::egui::{
    self, Align, Color32, Context, FontFamily, FontId, Frame, Layout, Margin, RichText,
    Stroke, Vec2,
};
use std::path::Path;

use crate::ui::theme::{medium, semibold, THEME};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    Play,
    Performance,
    Graphics,
    Shaders,
    Diagnostics,
    Settings,
}

impl Page {
    pub(crate) const ALL: [(Self, &'static str); 6] = [
        (Self::Play, "Home"),
        (Self::Performance, "Performance"),
        (Self::Graphics, "Graphics"),
        (Self::Shaders, "Shaders"),
        (Self::Diagnostics, "Diagnostics"),
        (Self::Settings, "Settings"),
    ];
}

pub(crate) fn load_horizon_logo(ctx: &Context) -> egui::TextureHandle {
    const LOGO: &[u8] = include_bytes!("../../horizon-source/launcher/assets/logos/grass-block.png");
    let image = match image::load_from_memory(LOGO) {
        Ok(image) => image.to_rgba8(),
        Err(error) => {
            eprintln!("Nebula: unable to load the Horizon logo: {error}");
            image::RgbaImage::from_pixel(1, 1, image::Rgba([123, 78, 245, 255]))
        }
    };
    let size = [image.width() as usize, image.height() as usize];
    let rgba = image.as_raw();
    ctx.load_texture(
        "horizon-grass-block",
        egui::ColorImage::from_rgba_unmultiplied(size, rgba),
        egui::TextureOptions::NEAREST,
    )
}

pub(crate) fn horizon_logo(ui: &mut egui::Ui, texture: &egui::TextureHandle, size: f32) {
    ui.add(
        egui::Image::new((texture.id(), Vec2::splat(size)))
            .corner_radius(egui::CornerRadius::same(8)),
    );
}

#[derive(Clone, Copy)]
pub(crate) enum WindowAction { Minimize, Maximize, Close }

pub(crate) fn window_control(ui: &mut egui::Ui, ctx: &Context, action: WindowAction) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(40.0, 32.0), egui::Sense::click());
    // Icon-only controls need a name for tooltips and screen readers.
    let response = response.on_hover_text(match action {
        WindowAction::Minimize => "Minimize",
        WindowAction::Maximize => "Maximize",
        WindowAction::Close => "Close",
    });
    let fill = match (action, response.hovered()) {
        (WindowAction::Close, true) => Color32::from_rgb(212, 70, 103),
        (_, true) => Color32::from_rgba_unmultiplied(255, 255, 255, 38),
        _ => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, 7.0, fill);
    let center = rect.center();
    let stroke = Stroke::new(1.4_f32, Color32::WHITE);
    match action {
        WindowAction::Minimize => { ui.painter().line_segment([center + Vec2::new(-5.0, 3.0), center + Vec2::new(5.0, 3.0)], stroke); }
        WindowAction::Maximize => {
            let r = egui::Rect::from_center_size(center, Vec2::splat(10.0));
            ui.painter().rect_stroke(r, 1.0, stroke, egui::StrokeKind::Inside);
        }
        WindowAction::Close => {
            ui.painter().line_segment([center + Vec2::splat(-4.0), center + Vec2::splat(4.0)], stroke);
            ui.painter().line_segment([center + Vec2::new(-4.0, 4.0), center + Vec2::new(4.0, -4.0)], stroke);
        }
    }
    if response.clicked() {
        match action {
            WindowAction::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            WindowAction::Maximize => {
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            WindowAction::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

pub(crate) fn toggle_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(46.0, 26.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        // Callers rely on `.changed()` to decide when to persist settings.
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *value, 0.14);
    let dim = if ui.is_enabled() { 1.0 } else { 0.4 };
    let track = lerp_color(Color32::from_rgb(54, 49, 68), THEME.primary, t).gamma_multiply(dim);
    if t > 0.0 {
        ui.painter().rect_filled(rect.expand(2.5), 15.5, THEME.primary.gamma_multiply(0.28 * t * dim));
    }
    ui.painter().rect_filled(rect, 13.0, track);
    ui.painter().rect_stroke(rect, 13.0, Stroke::new(1.0_f32, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
    let x = rect.left() + 13.0 + (rect.width() - 26.0) * t;
    ui.painter().circle_filled(egui::pos2(x, rect.center().y + 1.5), 10.0, Color32::from_black_alpha(45));
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 9.5, Color32::WHITE.gamma_multiply(dim));
    if response.has_focus() {
        ui.painter().rect_stroke(rect.expand(3.0), 16.0, Stroke::new(1.5_f32, THEME.primary_bright), egui::StrokeKind::Outside);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(crate) fn labeled_combo<'a>(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    id: &str,
    selected: &mut Option<usize>,
    choices: impl IntoIterator<Item = (usize, &'a str)>,
) {
    let choices: Vec<(usize, &'a str)> = choices.into_iter().collect();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).family(medium()).color(THEME.text));
            ui.label(RichText::new(description).size(12.5).color(THEME.muted));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let width = 330.0_f32.min(ui.available_width());
            dropdown(ui, egui::Id::new(("nebula_dropdown", id)), width, &choices, selected);
        });
    });
    ui.separator();
}

pub(crate) fn setting_row(
    ui: &mut egui::Ui,
    value: &mut bool,
    title: &str,
    description: &str,
    enabled: bool,
) -> bool {
    let changed = ui
        .horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .family(medium())
                        .color(if enabled { THEME.text } else { THEME.muted }),
                );
                ui.label(RichText::new(description).size(12.5).color(THEME.muted));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(enabled, |ui| toggle_switch(ui, value)).inner
            })
            .inner
            .changed()
        })
        .inner;
    ui.separator();
    changed
}

pub(crate) fn diagnostic_row(ui: &mut egui::Ui, label: &str, value: &Path) {
    ui.label(RichText::new(label).family(medium()).color(THEME.text));
    ui.label(
        RichText::new(value.display().to_string())
            .monospace()
            .size(12.5)
            .color(THEME.muted),
    );
    ui.add_space(8.0);
}

// ---------------------------------------------------------------------------------------------
// Visual components. Everything below is presentation only; launch logic lives elsewhere.
// ---------------------------------------------------------------------------------------------

/// Linear blend between two opaque colours.
pub(crate) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// The main translucent panel that holds a page. The sunset remains visible through it.
pub(crate) fn main_panel_frame(alpha: u8) -> Frame {
    Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(12, 9, 27, alpha))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(24)))
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(Margin::symmetric(28, 24))
        .shadow(egui::epaint::Shadow {
            offset: [0, 7],
            blur: 20,
            spread: 0,
            color: Color32::from_black_alpha(48),
        })
}

pub(crate) fn card_frame(highlight: bool) -> Frame {
    let (fill, border) = if highlight {
        (
            Color32::from_rgb(43, 32, 68),
            Color32::from_rgba_unmultiplied(167, 132, 246, 150),
        )
    } else {
        (
            Color32::from_rgb(25, 22, 39),
            Color32::from_rgb(58, 52, 76),
        )
    };
    Frame::NONE
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 13))
}

/// Renders a surface without adding decorative gradients or highlights.
pub(crate) fn panel_card<R>(
    ui: &mut egui::Ui,
    frame: Frame,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    frame.show(ui, add_contents).inner
}

pub(crate) fn accent_pill(ui: &mut egui::Ui) {
    let (pill, _) = ui.allocate_exact_size(Vec2::new(24.0, 2.0), egui::Sense::hover());
    ui.painter().rect_filled(pill, 1.0, THEME.primary);
    ui.add_space(7.0);
}

pub(crate) fn page_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
    accent_pill(ui);
    ui.label(RichText::new(eyebrow).size(12.0).family(semibold()).color(THEME.primary_bright));
    ui.add_space(4.0);
    ui.label(
        RichText::new(title)
            .font(FontId::new(32.0, semibold()))
            .color(THEME.text),
    );
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(THEME.muted));
    }
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);
}

pub(crate) fn account_chip(ui: &mut egui::Ui, account: Option<&str>) {
    Frame::NONE
        .fill(Color32::from_white_alpha(16))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(34)))
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (avatar, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                let initial = account
                    .and_then(|name| name.chars().next())
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string());
                ui.painter().circle_filled(
                    avatar.center(),
                    11.0,
                    if account.is_some() { THEME.primary } else { Color32::from_rgb(70, 62, 96) },
                );
                ui.painter().text(
                    avatar.center(),
                    egui::Align2::CENTER_CENTER,
                    initial,
                    FontId::new(12.0, medium()),
                    Color32::WHITE,
                );
                ui.label(RichText::new(account.unwrap_or("Account not loaded")).size(13.0).color(THEME.text));
            });
        });
}

/// One sidebar destination: painted icon, label, animated hover and selection states.
pub(crate) fn nav_item(ui: &mut egui::Ui, page: Page, label: &str, selected: bool, compact: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
    let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let active = ui.ctx().animate_bool_with_time(response.id.with("active"), selected, 0.18);
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, Color32::from_white_alpha((hover * 16.0) as u8));
    if active > 0.0 {
        painter.rect_filled(rect, 12.0, Color32::from_rgba_unmultiplied(139, 91, 249, (active * 110.0) as u8));
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(214, 190, 255, (active * 100.0) as u8)),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 12.0), Vec2::new(3.0, 20.0)),
            2.0,
            THEME.primary.gamma_multiply(active),
        );
    }
    let color = lerp_color(THEME.muted, THEME.text, active.max(hover * 0.7));
    let icon_x = if compact { rect.center().x } else { rect.left() + 26.0 };
    draw_nav_icon(painter, page, egui::pos2(icon_x, rect.center().y), color);
    if !compact {
        painter.text(
            egui::pos2(rect.left() + 50.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::new(14.5, medium()),
            color,
        );
    }
    if response.has_focus() {
        painter.rect_stroke(rect, 12.0, Stroke::new(1.5_f32, THEME.primary_bright), egui::StrokeKind::Inside);
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if compact {
        response.on_hover_text(label)
    } else {
        response
    }
}

/// A matched set of compact outline glyphs, drawn in the same 18px optical grid.
pub(crate) fn draw_nav_icon(painter: &egui::Painter, page: Page, c: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(1.8_f32, color);
    let p = |x: f32, y: f32| c + Vec2::new(x, y);
    match page {
        Page::Play => {
            painter.line_segment([p(-8.0, -1.0), p(0.0, -8.0)], stroke);
            painter.line_segment([p(0.0, -8.0), p(8.0, -1.0)], stroke);
            painter.line_segment([p(-6.0, -2.0), p(-6.0, 7.0)], stroke);
            painter.line_segment([p(6.0, -2.0), p(6.0, 7.0)], stroke);
            painter.line_segment([p(-6.0, 7.0), p(6.0, 7.0)], stroke);
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-1.8, 2.0), p(1.8, 7.0)),
                1.0,
                Stroke::new(1.5_f32, color),
                egui::StrokeKind::Inside,
            );
        }
        Page::Performance => {
            painter.line_segment([p(2.5, -9.0), p(-4.5, 1.0)], stroke);
            painter.line_segment([p(-4.5, 1.0), p(-0.5, 1.0)], stroke);
            painter.line_segment([p(-0.5, 1.0), p(-2.5, 9.0)], stroke);
            painter.line_segment([p(-2.5, 9.0), p(5.0, -2.0)], stroke);
            painter.line_segment([p(5.0, -2.0), p(1.0, -2.0)], stroke);
            painter.line_segment([p(1.0, -2.0), p(2.5, -9.0)], stroke);
        }
        Page::Graphics => {
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-8.5, -7.0), p(8.5, 4.5)),
                egui::CornerRadius::same(2),
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([p(0.0, 4.5), p(0.0, 8.0)], stroke);
            painter.line_segment([p(-4.5, 8.0), p(4.5, 8.0)], stroke);
        }
        Page::Shaders => {
            painter.line_segment([p(0.0, -8.5), p(0.0, 8.5)], stroke);
            painter.line_segment([p(-8.5, 0.0), p(8.5, 0.0)], stroke);
            painter.line_segment([p(-5.5, -5.5), p(5.5, 5.5)], stroke);
            painter.line_segment([p(5.5, -5.5), p(-5.5, 5.5)], stroke);
            painter.line_segment([p(7.0, -10.0), p(7.0, -4.0)], stroke);
            painter.line_segment([p(4.0, -7.0), p(10.0, -7.0)], stroke);
        }
        Page::Diagnostics => {
            painter.circle_stroke(p(-6.0, 0.0), 2.0, stroke);
            painter.line_segment([p(-3.0, 0.0), p(-0.5, 0.0)], stroke);
            painter.line_segment([p(-0.5, 0.0), p(2.0, -6.0)], stroke);
            painter.line_segment([p(2.0, -6.0), p(5.0, 6.0)], stroke);
            painter.line_segment([p(5.0, 6.0), p(7.0, 0.0)], stroke);
            painter.line_segment([p(7.0, 0.0), p(9.0, 0.0)], stroke);
        }
        Page::Settings => {
            for (y, knob) in [(-6.0_f32, -2.5_f32), (0.0, 3.0), (6.0, -1.5)] {
                painter.line_segment([p(-9.0, y), p(knob - 3.5, y)], stroke);
                painter.line_segment([p(knob + 3.5, y), p(9.0, y)], stroke);
                painter.circle_stroke(p(knob, y), 3.0, stroke);
            }
        }
    }
}
/// Returns true when it was clicked while enabled.
/// Primary launch action with a single brand surface and clear interaction states.
pub(crate) fn play_button(ui: &mut egui::Ui, enabled: bool, starting: bool) -> bool {
    let size = Vec2::new(ui.available_width().min(360.0), 56.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && enabled,
        0.14,
    );
    let pressed = enabled && response.is_pointer_button_down_on();
    let fill = if !enabled {
        Color32::from_rgb(52, 46, 68)
    } else if pressed {
        THEME.primary.gamma_multiply(0.86)
    } else {
        lerp_color(THEME.primary, THEME.primary_hover, hover)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(10), fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(10),
        Stroke::new(
            1.0_f32,
            if enabled {
                THEME.primary_bright.gamma_multiply(0.38 + hover * 0.18)
            } else {
                Color32::from_white_alpha(18)
            },
        ),
        egui::StrokeKind::Inside,
    );
    let font = FontId::new(17.0, semibold());
    let color = if enabled { Color32::WHITE } else { THEME.muted };
    if starting {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Starting Horizon…", font, color);
    } else {
        let text_width = painter.layout_no_wrap("Play".to_owned(), font.clone(), color).size().x;
        let icon_width = 17.0;
        let gap = 10.0;
        let left = rect.center().x - (icon_width + gap + text_width) / 2.0;
        paint_icon(
            painter,
            Icon::Play,
            egui::pos2(left + icon_width / 2.0, rect.center().y),
            icon_width,
            color,
            0.0,
        );
        painter.text(
            egui::pos2(left + icon_width + gap, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Play",
            font,
            color,
        );
    }
    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            egui::CornerRadius::same(12),
            Stroke::new(1.5_f32, THEME.primary_bright),
            egui::StrokeKind::Outside,
        );
    }
    let clicked = enabled && response.clicked();
    if enabled {
        let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    clicked
}

pub(crate) fn launch_progress(ui: &mut egui::Ui, time: f32) {
    let width = ui.available_width().min(360.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 4.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, Color32::from_white_alpha(24));
    let segment = rect.width() * 0.32;
    let phase = (time * 0.7).fract();
    let start = rect.left() - segment + (rect.width() + segment) * phase;
    let left = start.max(rect.left());
    let right = (start + segment).min(rect.right());
    if right > left {
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right, rect.bottom())),
            2.0,
            THEME.primary,
        );
    }
}

// <ui-logic>
// Layout and keyboard arithmetic for the dropdown and segmented control. Standard library only.

/// Where a dropdown list opens and how tall it may be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PopupPlan {
    /// True when the list opens under the field, false when it opens above.
    pub(crate) below: bool,
    /// Total height of the list panel, padding included.
    pub(crate) height: f32,
}

/// Opens below the field when the wanted height fits, otherwise on whichever side has more room,
/// and shortens the list (it then scrolls) when neither side fits it whole.
pub(crate) fn plan_popup(anchor_top: f32, anchor_bottom: f32, screen_top: f32, screen_bottom: f32, wanted: f32) -> PopupPlan {
    const GAP: f32 = 6.0;
    const EDGE: f32 = 12.0;
    const FLOOR: f32 = 120.0;
    let below_room = (screen_bottom - anchor_bottom - GAP - EDGE).max(0.0);
    let above_room = (anchor_top - screen_top - GAP - EDGE).max(0.0);
    let below = below_room >= wanted || below_room >= above_room;
    let room = if below { below_room } else { above_room };
    PopupPlan { below, height: wanted.min(room.max(FLOOR)) }
}

/// Moves the highlighted row by `delta` without wrapping. With nothing highlighted, a move down
/// starts at the first row and a move up at the last.
pub(crate) fn step_highlight(current: Option<usize>, len: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len as i64 - 1;
    Some(match current {
        None if delta >= 0 => 0,
        None => last as usize,
        Some(index) => (index as i64 + delta as i64).clamp(0, last) as usize,
    })
}

/// Width of one segment: the widest label plus padding, never narrower than a tap target and
/// shrunk when the row would not fit in `room`.
pub(crate) fn segment_width(widest_label: f32, count: usize, room: f32) -> f32 {
    let wanted = (widest_label + 28.0).max(64.0);
    let fit = room / count.max(1) as f32;
    wanted.min(fit.max(40.0))
}

/// Fast start, slow finish. Input is clamped to 0..=1.
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}
// </ui-logic>

// ---------------------------------------------------------------------------------------------
// Shared controls: buttons, dropdowns and segmented choices. Presentation only.
// Fields use inset surfaces; actions use a single raised surface level.
// ---------------------------------------------------------------------------------------------

/// Lays out single-line text and truncates it to the available width.
pub(crate) fn fitted_text(
    painter: &egui::Painter,
    text: &str,
    size: f32,
    family: FontFamily,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::text::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.to_owned(),
        FontId::new(size, family),
        color,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(8.0));
    painter.layout_job(job)
}

/// Small vector glyphs painted directly into egui surfaces.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Icon {
    Play,
    Refresh,
    Reset,
    Bolt,
    Power,
    Check,
    User,
}

pub(crate) fn arc_points(center: egui::Pos2, radius: f32, start: f32, sweep: f32, steps: usize) -> Vec<egui::Pos2> {
    (0..=steps)
        .map(|i| {
            let (sin, cos) = (start + sweep * i as f32 / steps as f32).sin_cos();
            center + Vec2::new(cos, sin) * radius
        })
        .collect()
}

pub(crate) fn rotated(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// Draws `icon` inside a `size` square centred on `c`. `turn` (radians) spins the circular arrows.
pub(crate) fn paint_icon(painter: &egui::Painter, icon: Icon, c: egui::Pos2, size: f32, color: Color32, turn: f32) {
    let k = size / 16.0;
    let stroke = Stroke::new(1.7 * k, color);
    let p = |x: f32, y: f32| c + Vec2::new(x * k, y * k);
    match icon {
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![p(-3.5, -6.0), p(-3.5, 6.0), p(6.0, 0.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Refresh | Icon::Reset => {
            let dir = if icon == Icon::Refresh { 1.0_f32 } else { -1.0 };
            let base = if icon == Icon::Refresh { -0.87_f32 } else { -std::f32::consts::PI + 0.87 };
            let start = base + dir * turn;
            let sweep = dir * 4.9;
            painter.add(egui::Shape::line(arc_points(c, 5.6 * k, start, sweep, 28), stroke));
            let end = start + sweep;
            let (sin, cos) = end.sin_cos();
            let tip = c + Vec2::new(cos, sin) * 5.6 * k;
            let forward = Vec2::new(-sin, cos) * dir;
            let tip = tip + forward * 2.0 * k;
            for side in [-0.6_f32, 0.6] {
                painter.line_segment([tip, tip - rotated(forward, side) * 4.4 * k], stroke);
            }
        }
        Icon::Bolt => {
            painter.add(egui::Shape::line(
                vec![p(2.5, -8.0), p(-4.0, 1.0), p(4.0, 1.0), p(-2.5, 8.0)],
                stroke,
            ));
        }
        Icon::Power => {
            painter.add(egui::Shape::line(arc_points(c, 5.6 * k, -0.87, 4.9, 28), stroke));
            painter.line_segment([p(0.0, -7.5), p(0.0, -1.0)], stroke);
        }
        Icon::Check => {
            painter.add(egui::Shape::line(vec![p(-5.0, 0.5), p(-1.5, 4.0), p(5.5, -4.0)], stroke));
        }
        Icon::User => {
            painter.circle_stroke(p(0.0, -3.5), 3.2 * k, stroke);
            painter.add(egui::Shape::line(arc_points(p(0.0, 9.0), 6.8 * k, -2.55, 1.96, 14), stroke));
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ButtonKind {
    /// Solid Nebula-purple primary action.
    Primary,
    /// Low-emphasis filled surface for secondary actions.
    Secondary,
    /// Text-only utility action with a subtle hover surface.
    Ghost,
}

const BUTTON_HEIGHT: f32 = 40.0;
const BUTTON_RADIUS: u8 = 12;
const BUTTON_ICON: f32 = 16.0;

/// Draws a consistent flat button with restrained hover and keyboard-focus states.
pub(crate) fn action_button(
    ui: &mut egui::Ui,
    kind: ButtonKind,
    icon: Option<Icon>,
    label: &str,
    min_width: f32,
) -> egui::Response {
    let enabled = ui.is_enabled();
    let font = FontId::new(14.5, medium());
    let text_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x;
    let icon_room = if icon.is_some() { BUTTON_ICON + 9.0 } else { 0.0 };
    let width = (text_width + icon_room + 32.0).max(min_width);
    let (full, response) = ui.allocate_exact_size(Vec2::new(width, BUTTON_HEIGHT), egui::Sense::click());
    let ctx = ui.ctx().clone();
    let hovered = enabled && response.hovered();
    let hover = ctx.animate_bool_with_time(response.id.with("hover"), hovered, 0.14);
    let pressed = enabled && response.is_pointer_button_down_on();
    let rect = if pressed { full.shrink(1.0) } else { full };
    let radius = egui::CornerRadius::same(BUTTON_RADIUS);

    let fill = match (kind, enabled) {
        (ButtonKind::Primary, true) => {
            let color = lerp_color(THEME.primary, THEME.primary_hover, hover);
            if pressed { color.gamma_multiply(0.88) } else { color }
        }
        (ButtonKind::Primary, false) => Color32::from_rgb(52, 46, 68),
        (ButtonKind::Secondary, true) if hovered => THEME.primary_soft,
        (ButtonKind::Secondary, true) => THEME.surface_raised,
        (ButtonKind::Secondary, false) => THEME.surface,
        (ButtonKind::Ghost, true) => Color32::from_white_alpha((14.0 * hover) as u8),
        (ButtonKind::Ghost, false) => Color32::TRANSPARENT,
    };
    let text_color = match (kind, enabled) {
        (ButtonKind::Primary, true) => Color32::WHITE,
        (ButtonKind::Primary, false) => THEME.muted,
        (ButtonKind::Secondary, true) => THEME.text,
        (ButtonKind::Secondary, false) => THEME.muted.gamma_multiply(0.7),
        (ButtonKind::Ghost, true) => lerp_color(THEME.muted, THEME.text, hover),
        (ButtonKind::Ghost, false) => THEME.muted.gamma_multiply(0.5),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, radius, fill);
    if kind != ButtonKind::Ghost || hovered {
        let border = match kind {
            ButtonKind::Primary if enabled => THEME.primary_bright.gamma_multiply(0.36),
            ButtonKind::Secondary if hovered => THEME.primary.gamma_multiply(0.52),
            ButtonKind::Secondary => THEME.border,
            ButtonKind::Ghost => Color32::from_white_alpha((28.0 * hover) as u8),
            ButtonKind::Primary => THEME.border,
        };
        painter.rect_stroke(rect, radius, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
    }

    let spin_key = response.id.with("spin");
    let now = ctx.input(|input| input.time);
    let is_refresh = matches!(icon, Some(Icon::Refresh));
    if is_refresh && enabled && response.clicked() {
        ctx.data_mut(|data| data.insert_temp(spin_key, now));
    }
    let turn = if is_refresh {
        let elapsed = ctx
            .data(|data| data.get_temp::<f64>(spin_key))
            .map_or(1.0, |start| ((now - start) / 0.7).clamp(0.0, 1.0) as f32);
        hover * 0.18 + ease_out_cubic(elapsed) * std::f32::consts::TAU
    } else {
        0.0
    };

    let left = rect.center().x - (text_width + icon_room) / 2.0;
    if let Some(icon) = icon {
        paint_icon(
            painter,
            icon,
            egui::pos2(left + BUTTON_ICON / 2.0, rect.center().y),
            BUTTON_ICON,
            text_color,
            turn,
        );
    }
    painter.text(
        egui::pos2(left + icon_room, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        font,
        text_color,
    );
    if response.has_focus() {
        painter.rect_stroke(
            full.expand(2.0),
            egui::CornerRadius::same(BUTTON_RADIUS + 2),
            Stroke::new(1.5_f32, THEME.primary_bright),
            egui::StrokeKind::Outside,
        );
    }
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

const FIELD_HEIGHT: f32 = 42.0;
const LIST_ROW: f32 = 38.0;
const LIST_PAD: f32 = 6.0;
const LIST_MAX: f32 = 300.0;

/// Opaque dropdown surface that keeps choices readable over the background artwork.
pub(crate) fn popup_frame() -> Frame {
    Frame::NONE
        .fill(THEME.surface_raised)
        .stroke(Stroke::new(1.0_f32, THEME.border))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(Margin::same(6))
        .shadow(egui::epaint::Shadow {
            offset: [0, 10],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(120),
        })
}

pub(crate) fn dropdown_row(
    ui: &mut egui::Ui,
    text: &str,
    selected: bool,
    highlighted: bool,
    interactive: bool,
) -> egui::Response {
    let sense = if interactive { egui::Sense::CLICK } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), LIST_ROW), sense);
    let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), highlighted, 0.10);
    let painter = ui.painter();
    let inner = rect.shrink2(Vec2::new(0.0, 1.0));
    let radius = egui::CornerRadius::same(8);
    if selected || hover > 0.0 {
        let alpha = if selected { 34 } else { (46.0 * hover) as u8 };
        painter.rect_filled(inner, radius, Color32::from_rgba_unmultiplied(123, 78, 245, alpha));
    }
    if selected {
        painter.rect_filled(
            egui::Rect::from_min_size(inner.left_top() + Vec2::new(0.0, 9.0), Vec2::new(2.0, inner.height() - 18.0)),
            1.0,
            THEME.primary,
        );
    }
    let color = if selected || hover > 0.0 { THEME.text } else { THEME.muted };
    let galley = fitted_text(painter, text, 14.5, FontFamily::Proportional, color, inner.width() - 60.0);
    painter.galley(
        egui::pos2(inner.left() + 14.0, inner.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if selected {
        paint_icon(
            painter,
            Icon::Check,
            egui::pos2(inner.right() - 20.0, inner.center().y),
            15.0,
            THEME.primary_bright,
            0.0,
        );
    }
    if interactive {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

pub(crate) fn dropdown(
    ui: &mut egui::Ui,
    id: egui::Id,
    width: f32,
    choices: &[(usize, &str)],
    selected: &mut Option<usize>,
) -> bool {
    let ctx = ui.ctx().clone();
    let enabled = ui.is_enabled() && !choices.is_empty();
    let current = *selected;
    let selected_position = current.and_then(|value| choices.iter().position(|(index, _)| *index == value));
    let label = if choices.is_empty() {
        "None available"
    } else {
        selected_position.map_or("Not found", |position| choices[position].1)
    };

    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, FIELD_HEIGHT), egui::Sense::click());
    let open_key = id.with("open");
    let highlight_key = id.with("highlight");
    let scroll_key = id.with("scroll_pending");
    let mut open = enabled && ctx.data(|data| data.get_temp::<bool>(open_key)).unwrap_or(false);
    let mut highlight: Option<usize> = ctx.data(|data| data.get_temp::<Option<usize>>(highlight_key)).flatten();
    let mut changed = false;
    let mut keep_focus = true;
    let mut just_opened = false;
    let mut moved_by_keys = false;

    // Arrow keys and Escape belong to the field while it has focus, so focus does not wander off.
    if enabled && response.has_focus() {
        ctx.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                egui::EventFilter { vertical_arrows: true, escape: true, ..Default::default() },
            )
        });
    }
    let key_activate = response.has_focus()
        && ctx.input(|input| input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space));
    let arrow_open = enabled
        && !open
        && response.has_focus()
        && ctx.input_mut(|input| {
            let none = egui::Modifiers::NONE;
            let down = input.consume_key(none, egui::Key::ArrowDown);
            let up = input.consume_key(none, egui::Key::ArrowUp);
            down || up
        });
    if enabled && ((response.clicked() && !(open && key_activate)) || arrow_open) {
        open = !open;
        if open {
            just_opened = true;
            highlight = selected_position.or(Some(0));
            ctx.data_mut(|data| data.insert_temp(scroll_key, true));
            response.request_focus();
        } else {
            keep_focus = false;
        }
    }

    if open && !just_opened {
        let (up, down, home, end, enter, space, escape, tab) = ctx.input_mut(|input| {
            let none = egui::Modifiers::NONE;
            (
                input.consume_key(none, egui::Key::ArrowUp),
                input.consume_key(none, egui::Key::ArrowDown),
                input.consume_key(none, egui::Key::Home),
                input.consume_key(none, egui::Key::End),
                input.consume_key(none, egui::Key::Enter),
                input.consume_key(none, egui::Key::Space),
                input.consume_key(none, egui::Key::Escape),
                input.key_pressed(egui::Key::Tab),
            )
        });
        if up {
            highlight = step_highlight(highlight, choices.len(), -1);
        }
        if down {
            highlight = step_highlight(highlight, choices.len(), 1);
        }
        if home {
            highlight = Some(0);
        }
        if end {
            highlight = choices.len().checked_sub(1);
        }
        moved_by_keys = up || down || home || end;
        if enter || space || key_activate {
            if let Some(position) = highlight.filter(|position| *position < choices.len()) {
                *selected = Some(choices[position].0);
                changed = true;
            }
            open = false;
        } else if escape || tab {
            open = false;
        }
    }

    let t = ctx.animate_bool_with_time(id.with("anim"), open, 0.16);
    if t > 0.0 {
        let interactive = open;
        let fade = ease_out_cubic(t);
        let wanted = (choices.len() as f32 * LIST_ROW + 2.0 * LIST_PAD).min(LIST_MAX + 2.0 * LIST_PAD);
        let screen = ctx.screen_rect();
        let plan = plan_popup(rect.top(), rect.bottom(), screen.top(), screen.bottom(), wanted);
        let slide = (1.0 - fade) * 8.0;
        let (pos, pivot) = if plan.below {
            (egui::pos2(rect.left(), rect.bottom() + 6.0 - slide), egui::Align2::LEFT_TOP)
        } else {
            (egui::pos2(rect.left(), rect.top() - 6.0 + slide), egui::Align2::LEFT_BOTTOM)
        };
        let pointer_moved = ctx.input(|input| input.pointer.delta() != Vec2::ZERO);
        let scroll_pending = ctx.data(|data| data.get_temp::<bool>(scroll_key)).unwrap_or(false);
        let scroll_to_highlight = moved_by_keys || scroll_pending;
        let mut clicked: Option<usize> = None;
        let mut hovered: Option<usize> = None;
        let area = egui::Area::new(id.with("popup"))
            .order(egui::Order::Foreground)
            .interactable(interactive)
            .pivot(pivot)
            .fixed_pos(pos)
            .show(&ctx, |ui| {
                ui.set_opacity(fade);
                panel_card(ui, popup_frame(), |ui| {
                    ui.set_width(width - 2.0 * LIST_PAD);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("scroll"))
                        .max_height(plan.height - 2.0 * LIST_PAD)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for (position, (index, text)) in choices.iter().enumerate() {
                                let row = dropdown_row(
                                    ui,
                                    text,
                                    current == Some(*index),
                                    highlight == Some(position),
                                    interactive,
                                );
                                if row.hovered() {
                                    hovered = Some(position);
                                }
                                if row.clicked() {
                                    clicked = Some(position);
                                }
                                if scroll_to_highlight && highlight == Some(position) {
                                    row.scroll_to_me(None);
                                }
                            }
                        });
                });
            });
        if scroll_pending {
            ctx.data_mut(|data| data.insert_temp(scroll_key, false));
        }
        if open {
            if let Some(position) = hovered {
                if pointer_moved {
                    highlight = Some(position);
                }
            }
            let popup_rect = area.response.rect;
            let pressed_outside = ctx.input(|input| {
                input.pointer.any_pressed()
                    && input
                        .pointer
                        .interact_pos()
                        .map_or(false, |at| !popup_rect.contains(at) && !rect.contains(at))
            });
            if pressed_outside {
                open = false;
                keep_focus = false;
            }
            if let Some(position) = clicked {
                *selected = Some(choices[position].0);
                changed = true;
                open = false;
                keep_focus = false;
            }
        }
    }
    if !open && !keep_focus {
        // After a mouse pick, no focus ring should linger on the field.
        response.surrender_focus();
    }
    ctx.data_mut(|data| {
        data.insert_temp(open_key, open);
        data.insert_temp(highlight_key, highlight);
    });

    let hover = ctx.animate_bool_with_time(id.with("hover"), enabled && response.hovered(), 0.14);
    let painter = ui.painter();
    let radius = egui::CornerRadius::same(10);
    let dim = if enabled { 1.0 } else { 0.5 };
    let fill = if !enabled {
        THEME.surface
    } else if open || hover > 0.0 {
        THEME.primary_soft
    } else {
        THEME.surface_raised
    };
    let border = if response.has_focus() || open {
        THEME.primary_bright.gamma_multiply(0.6)
    } else if hover > 0.0 {
        THEME.primary.gamma_multiply(0.5)
    } else {
        THEME.border
    };
    painter.rect_filled(rect, radius, fill);
    painter.rect_stroke(rect, radius, Stroke::new(1.0_f32, border), egui::StrokeKind::Inside);
    let text_color = if enabled { THEME.text.gamma_multiply(dim) } else { THEME.muted };
    let galley = fitted_text(painter, label, 14.5, medium(), text_color, rect.width() - 56.0);
    painter.galley(
        egui::pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    let chevron = egui::pos2(rect.right() - 20.0, rect.center().y);
    let flip = if open { -1.0 } else { 1.0 };
    painter.add(egui::Shape::line(
        vec![
            chevron + Vec2::new(-4.0, -1.8 * flip),
            chevron + Vec2::new(0.0, 1.8 * flip),
            chevron + Vec2::new(4.0, -1.8 * flip),
        ],
        Stroke::new(1.5_f32, THEME.muted.gamma_multiply(dim)),
    ));
    if response.has_focus() && !open {
        painter.rect_stroke(
            rect.expand(2.5),
            egui::CornerRadius::same(14),
            Stroke::new(1.5_f32, THEME.primary_bright),
            egui::StrokeKind::Outside,
        );
    }
    if enabled && response.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    changed
}

const SLIDER_MAX_WIDTH: f32 = 380.0;

/// A whole-number slider with a single brand-purple track and a compact value label.
/// The value changes live while dragging. Returns true when a value is final: the pointer was
/// released, or an arrow key was pressed (Shift moves by ten).
pub(crate) fn integer_slider(
    ui: &mut egui::Ui,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    suffix: &str,
) -> bool {
    let (low, high) = (*range.start(), *range.end());
    let enabled = ui.is_enabled();
    let ctx = ui.ctx().clone();
    let chip_width = 88.0;
    let track_width = (ui.available_width() - chip_width - 16.0).clamp(160.0, SLIDER_MAX_WIDTH);
    let mut committed = false;
    ui.horizontal(|ui| {
        let (area, response) = ui.allocate_exact_size(Vec2::new(track_width, 34.0), egui::Sense::click_and_drag());
        // The thumb stays inside the area at both ends.
        let margin = 11.0;
        let left = area.left() + margin;
        let span = (area.width() - 2.0 * margin).max(1.0);
        let steps = (high - low).max(1) as f32;
        if enabled && (response.is_pointer_button_down_on() || response.dragged() || response.clicked()) {
            if let Some(at) = response.interact_pointer_pos() {
                let fraction = ((at.x - left) / span).clamp(0.0, 1.0);
                *value = low + (fraction * steps).round() as u32;
            }
        }
        if enabled && (response.drag_stopped() || response.clicked()) {
            committed = true;
        }
        if enabled && response.has_focus() {
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter { horizontal_arrows: true, ..Default::default() },
                )
            });
            let step: i64 = ctx.input_mut(|input| {
                let none = egui::Modifiers::NONE;
                let shift = egui::Modifiers::SHIFT;
                if input.consume_key(shift, egui::Key::ArrowRight) {
                    10
                } else if input.consume_key(shift, egui::Key::ArrowLeft) {
                    -10
                } else if input.consume_key(none, egui::Key::ArrowRight) {
                    1
                } else if input.consume_key(none, egui::Key::ArrowLeft) {
                    -1
                } else {
                    0
                }
            });
            if step != 0 {
                *value = (*value as i64 + step).clamp(low as i64, high as i64) as u32;
                committed = true;
            }
        }
        *value = (*value).clamp(low, high);
        let hover = ctx.animate_bool_with_time(
            response.id.with("hover"),
            enabled && (response.hovered() || response.dragged()),
            0.14,
        );
        let fraction = (*value - low) as f32 / steps;
        let dim = if enabled { 1.0 } else { 0.45 };
        let fill_color = THEME.primary.gamma_multiply(dim);
        let painter = ui.painter();
        let rail = egui::Rect::from_min_max(
            egui::pos2(left, area.center().y - 3.0),
            egui::pos2(left + span, area.center().y + 3.0),
        );
        painter.rect_filled(rail, 3.0, Color32::from_rgba_unmultiplied(8, 5, 22, 150));
        painter.rect_stroke(rail, 3.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
        let thumb_x = left + span * fraction;
        let filled = egui::Rect::from_min_max(rail.left_top(), egui::pos2(thumb_x, rail.bottom()));
        if filled.width() > 0.5 {
            painter.rect_filled(filled, 3.0, fill_color);
        }
        let centre = egui::pos2(thumb_x, area.center().y);
        painter.circle_filled(centre + Vec2::new(0.0, 1.0), 10.0, Color32::from_black_alpha(45));
        painter.circle_filled(centre, 9.5, if enabled { Color32::WHITE } else { Color32::from_rgb(150, 142, 172) });
        painter.circle_filled(centre, 3.5, if enabled { THEME.primary } else { Color32::from_rgb(78, 66, 112) });
        if enabled && hover > 0.0 {
            painter.circle_stroke(
                centre,
                12.0 + hover,
                Stroke::new(1.0_f32, THEME.primary.gamma_multiply(0.4)),
            );
        }
        if response.has_focus() {
            painter.circle_stroke(centre, 14.5, Stroke::new(1.5_f32, THEME.primary_bright));
        }
        if enabled && (response.hovered() || response.dragged()) {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let (chip, _) = ui.allocate_exact_size(Vec2::new(chip_width, 30.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(chip, 8.0, THEME.surface_raised);
        painter.rect_stroke(chip, 8.0, Stroke::new(1.0_f32, THEME.border), egui::StrokeKind::Inside);
        painter.text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}{}", *value, suffix),
            FontId::new(13.5, medium()),
            if enabled { THEME.text } else { THEME.muted.gamma_multiply(0.6) },
        );
    });
    committed
}

const TRACK_HEIGHT: f32 = 42.0;
const TRACK_INSET: f32 = 4.0;

/// A row of mutually exclusive choices in an inset track, with a thumb that slides to the pick.
/// Returns the value that was clicked, when it differs from `current`.
/// A row of mutually exclusive choices under a title and description.
/// Returns the newly picked value, if any.
pub(crate) fn choice_row(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    options: &[(&str, &str)],
    current: &str,
) -> Option<String> {
    ui.label(RichText::new(title).family(medium()).color(THEME.text));
    ui.label(RichText::new(description).size(12.5).color(THEME.muted));
    ui.add_space(2.0);
    let picked = segmented_control(ui, egui::Id::new(("nebula_choice", title)), options, current);
    ui.separator();
    picked
}


pub(crate) fn segmented_control(
    ui: &mut egui::Ui,
    id: egui::Id,
    options: &[(&str, &str)],
    current: &str,
) -> Option<String> {
    let ctx = ui.ctx().clone();
    let enabled = ui.is_enabled();
    let widest = options
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap((*label).to_owned(), FontId::new(13.5, medium()), Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let count = options.len().max(1);
    let segment = segment_width(widest, count, ui.available_width() - 2.0 * TRACK_INSET);
    let size = Vec2::new(segment * count as f32 + 2.0 * TRACK_INSET, TRACK_HEIGHT);
    let (track, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let current_index = options.iter().position(|(value, _)| *value == current);

    // One interaction area per segment. They are collected before painting so the thumb and
    // labels can be drawn in a fixed order.
    let responses: Vec<egui::Response> = (0..options.len())
        .map(|i| {
            let cell = egui::Rect::from_min_size(
                track.left_top() + Vec2::new(TRACK_INSET + segment * i as f32, TRACK_INSET),
                Vec2::new(segment, TRACK_HEIGHT - 2.0 * TRACK_INSET),
            );
            ui.interact(cell, id.with(i), egui::Sense::click())
        })
        .collect();
    let hovers: Vec<f32> = responses
        .iter()
        .enumerate()
        .map(|(i, response)| {
            ctx.animate_bool_with_time(
                response.id.with("hover"),
                enabled && response.hovered() && current_index != Some(i),
                0.12,
            )
        })
        .collect();
    let slot = ctx.animate_value_with_time(id.with("thumb"), current_index.unwrap_or(0) as f32, 0.2);
    let has_pick = ctx.animate_bool_with_time(id.with("has"), current_index.is_some(), 0.15);
    let dim = if enabled { 1.0 } else { 0.5 };

    let painter = ui.painter();
    let track_radius = egui::CornerRadius::same(10);
    painter.rect_filled(track, track_radius, THEME.surface_raised);
    painter.rect_stroke(
        track,
        track_radius,
        Stroke::new(1.0_f32, THEME.border),
        egui::StrokeKind::Inside,
    );
    for (response, hover) in responses.iter().zip(&hovers) {
        if *hover > 0.0 {
            painter.rect_filled(
                response.rect,
                egui::CornerRadius::same(8),
                Color32::from_white_alpha((12.0 * hover) as u8),
            );
        }
    }
    if has_pick > 0.0 {
        let thumb = egui::Rect::from_min_size(
            track.left_top() + Vec2::new(TRACK_INSET + segment * slot, TRACK_INSET),
            Vec2::new(segment, TRACK_HEIGHT - 2.0 * TRACK_INSET),
        );
        painter.rect_filled(
            thumb,
            egui::CornerRadius::same(8),
            THEME.primary.gamma_multiply(dim),
        );
        painter.rect_stroke(
            thumb,
            egui::CornerRadius::same(8),
            Stroke::new(1.0_f32, THEME.primary_bright.gamma_multiply(0.32)),
            egui::StrokeKind::Inside,
        );
    }
    for (i, (_, label)) in options.iter().enumerate() {
        let cell = responses[i].rect;
        // The label brightens as the thumb slides under it.
        let near = (1.0 - (slot - i as f32).abs()).clamp(0.0, 1.0) * has_pick;
        let base = lerp_color(Color32::from_rgb(190, 183, 212), THEME.text, hovers[i]);
        let color = lerp_color(base, Color32::WHITE, near).gamma_multiply(dim);
        let galley = fitted_text(painter, label, 13.5, medium(), color, cell.width() - 14.0);
        painter.galley(
            egui::pos2(cell.center().x - galley.size().x / 2.0, cell.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
        if responses[i].has_focus() {
            painter.rect_stroke(
                cell,
                egui::CornerRadius::same(9),
                Stroke::new(1.5_f32, THEME.primary_bright),
                egui::StrokeKind::Inside,
            );
        }
    }
    let mut picked = None;
    for (i, response) in responses.iter().enumerate() {
        if enabled && response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if enabled && response.clicked() && current_index != Some(i) {
            picked = Some(options[i].0.to_owned());
        }
    }
    picked
}

// <settings-logic>
