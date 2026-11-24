//! Reusable controls and their layout arithmetic.

use crate::theme::*;
use eframe::egui::{self, Color32, FontFamily, FontId, Frame, Margin, RichText, Stroke, Vec2};

pub(crate) fn toggle_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(46.0, 26.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        // Callers rely on `.changed()` to decide when to persist settings.
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *value, 0.14);
    let dim = if ui.is_enabled() { 1.0 } else { 0.4 };
    let track = lerp_color(Color32::from_rgb(54, 49, 68), VIOLET, t).gamma_multiply(dim);
    if t > 0.0 {
        ui.painter().rect_filled(rect.expand(2.5), 15.5, VIOLET.gamma_multiply(0.28 * t * dim));
    }
    ui.painter().rect_filled(rect, 13.0, track);
    ui.painter().rect_stroke(rect, 13.0, Stroke::new(1.0_f32, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
    let x = rect.left() + 13.0 + (rect.width() - 26.0) * t;
    ui.painter().circle_filled(egui::pos2(x, rect.center().y + 1.5), 10.0, Color32::from_black_alpha(45));
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 9.5, Color32::WHITE.gamma_multiply(dim));
    if response.has_focus() {
        ui.painter().rect_stroke(rect.expand(3.0), 16.0, Stroke::new(1.5_f32, PURPLE_BRIGHT), egui::StrokeKind::Outside);
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(crate) fn glass_card<R>(ui: &mut egui::Ui, frame: Frame, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    frame.show(ui, add_contents).inner
}

pub(crate) fn page_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(RichText::new(eyebrow).size(12.0).family(semibold()).color(CORAL_TEXT));
    ui.add_space(4.0);
    ui.label(
        RichText::new(title)
            .font(FontId::new(32.0, semibold()))
            .color(TEXT),
    );
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(MUTED));
    }
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);
}

/// A quiet status pill: a coloured dot, the state and the selected build.
pub(crate) fn status_chip(ui: &mut egui::Ui, state: &str, color: Color32, detail: &str) {
    Frame::NONE
        .fill(Color32::from_white_alpha(12))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(26)))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(Margin::symmetric(12, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, color);
                ui.label(RichText::new(state).size(13.0).family(medium()).color(TEXT));
                ui.label(RichText::new("·").size(13.0).color(MUTED));
                ui.label(RichText::new(detail).size(13.0).color(MUTED));
            });
        });
}


// <ui-logic>
// Layout and keyboard arithmetic for the dropdown and segmented control. Standard library only.

/// Where a dropdown list opens and how tall it may be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PopupPlan {
    /// True when the list opens under its field, false when it opens above.
    below: bool,
    /// Total height of the list panel, padding included.
    height: f32,
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
// Premium controls: buttons, dropdown and segmented choice. Presentation only.
// Inset glass wells hold values (dropdown field, choice track); raised glass triggers actions.
// ---------------------------------------------------------------------------------------------

/// Blend that also blends transparency (`lerp_color` is for opaque colours only).
pub(crate) fn lerp_rgba(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2]), mix(a[3], b[3]))
}

/// One line of text that ends in an ellipsis instead of running past `max_width`.
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

/// A horizontal or vertical gradient inside a rounded rectangle. Flat end caps carry the rounded
/// corners and a mesh fills the span between them. Colours must be opaque so the pieces never
/// blend over each other.
pub(crate) fn paint_round_gradient(
    painter: &egui::Painter,
    rect: egui::Rect,
    r: u8,
    from: Color32,
    to: Color32,
    vertical: bool,
) {
    let rf = r as f32;
    if vertical {
        painter.rect_filled(
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), rect.top() + rf)),
            egui::CornerRadius { nw: r, ne: r, sw: 0, se: 0 },
            from,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - rf), rect.right_bottom()),
            egui::CornerRadius { nw: 0, ne: 0, sw: r, se: r },
            to,
        );
        let span = egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + rf),
            egui::pos2(rect.right(), rect.bottom() - rf),
        );
        paint_gradient(painter, span, from, from, to, to);
    } else {
        painter.rect_filled(
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + rf, rect.bottom())),
            egui::CornerRadius { nw: r, ne: 0, sw: r, se: 0 },
            from,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.right() - rf, rect.top()), rect.right_bottom()),
            egui::CornerRadius { nw: 0, ne: r, sw: 0, se: r },
            to,
        );
        let span = egui::Rect::from_min_max(
            egui::pos2(rect.left() + rf, rect.top()),
            egui::pos2(rect.right() - rf, rect.bottom()),
        );
        paint_gradient(painter, span, from, to, to, from);
    }
}

/// Glyphs drawn with the painter, so they stay crisp and need no icon font.
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

pub(crate) fn rotated(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// Draws `icon` inside a `size` square centred on `c`. `turn` (radians) spins the circular arrows.
pub(crate) fn paint_icon(painter: &egui::Painter, icon: Icon, c: egui::Pos2, size: f32, color: Color32, turn: f32) {
    let k = size / 16.0;
    let stroke = Stroke::new(1.8 * k, color);
    let p = |x: f32, y: f32| c + Vec2::new(x * k, y * k);
    match icon {
        Icon::Play => {
            painter.line_segment([p(-3.5, -5.5), p(5.5, 0.0)], stroke);
            painter.line_segment([p(5.5, 0.0), p(-3.5, 5.5)], stroke);
            painter.line_segment([p(-3.5, 5.5), p(-3.5, -5.5)], stroke);
        }
        Icon::Refresh | Icon::Reset => {
            let dir = if icon == Icon::Refresh { 1.0_f32 } else { -1.0 };
            let base = if icon == Icon::Refresh { -0.87_f32 } else { -std::f32::consts::PI + 0.87 };
            let start = base + dir * turn;
            let sweep = dir * 4.9;
            let steps = 24;
            let mut previous = c + Vec2::new(start.cos(), start.sin()) * 5.6 * k;
            for step in 1..=steps {
                let angle = start + sweep * step as f32 / steps as f32;
                let current = c + Vec2::new(angle.cos(), angle.sin()) * 5.6 * k;
                painter.line_segment([previous, current], stroke);
                previous = current;
            }
            let end = start + sweep;
            let tangent = Vec2::new(-end.sin(), end.cos()) * dir;
            let tip = c + Vec2::new(end.cos(), end.sin()) * 5.6 * k + tangent * 2.0 * k;
            let side = rotated(tangent, 0.65) * 4.0 * k;
            let other = rotated(tangent, -0.65) * 4.0 * k;
            painter.line_segment([tip - side, tip], stroke);
            painter.line_segment([tip - other, tip], stroke);
        }
        Icon::Bolt => {
            painter.line_segment([p(2.5, -8.0), p(-4.0, 1.0)], stroke);
            painter.line_segment([p(-4.0, 1.0), p(4.0, 1.0)], stroke);
            painter.line_segment([p(4.0, 1.0), p(-2.5, 8.0)], stroke);
        }
        Icon::Power => {
            let start = -0.87_f32;
            let sweep = 4.9_f32;
            let steps = 24;
            let mut previous = p(5.6 * start.cos(), 5.6 * start.sin());
            for step in 1..=steps {
                let angle = start + sweep * step as f32 / steps as f32;
                let current = p(5.6 * angle.cos(), 5.6 * angle.sin());
                painter.line_segment([previous, current], stroke);
                previous = current;
            }
            painter.line_segment([p(0.0, -7.5), p(0.0, -1.0)], stroke);
        }
        Icon::Check => {
            painter.line_segment([p(-5.0, 0.5), p(-1.5, 4.0)], stroke);
            painter.line_segment([p(-1.5, 4.0), p(5.5, -4.0)], stroke);
        }
        Icon::User => {
            painter.circle_stroke(p(0.0, -3.5), 3.2 * k, stroke);
            let center = p(0.0, 9.0);
            let radius = 6.8 * k;
            let start = -2.55_f32;
            let sweep = 1.96_f32;
            let steps = 12;
            let mut previous = center + Vec2::new(start.cos(), start.sin()) * radius;
            for step in 1..=steps {
                let angle = start + sweep * step as f32 / steps as f32;
                let current = center + Vec2::new(angle.cos(), angle.sin()) * radius;
                painter.line_segment([previous, current], stroke);
                previous = current;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ButtonKind {
    /// Sunset gradient: the main action of a card.
    Primary,
    /// Frosted violet glass: the everyday button.
    Glass,
    /// Text only until hovered: low-emphasis utilities.
    Ghost,
}

pub(crate) const BUTTON_HEIGHT: f32 = 40.0;
pub(crate) const BUTTON_RADIUS: u8 = 10;
pub(crate) const BUTTON_ICON: f32 = 16.0;

/// The one button style of the launcher: eased hover, press-in, light sweep, painted icon and a
/// keyboard focus ring. Disabled looks come from the surrounding `ui` being disabled.
pub(crate) fn premium_button(
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
    let width = (text_width + icon_room + 40.0).max(min_width);
    let (full, response) = ui.allocate_exact_size(Vec2::new(width, BUTTON_HEIGHT), egui::Sense::click());
    let ctx = ui.ctx().clone();
    let hovered = enabled && response.hovered();
    let hover = ctx.animate_bool_with_time(response.id.with("hover"), hovered, 0.14);
    let pressed = enabled && response.is_pointer_button_down_on();
    let rect = if pressed { full.shrink(1.0) } else { full };
    let r = BUTTON_RADIUS;
    let radius = egui::CornerRadius::same(r);

    // Circular arrows nudge on hover and spin once when clicked.
    let spin_key = response.id.with("spin");
    let now = ctx.input(|input| input.time);
    if enabled && response.clicked() {
        ctx.data_mut(|data| data.insert_temp(spin_key, now));
    }
    let spun = ctx
        .data(|data| data.get_temp::<f64>(spin_key))
        .map_or(1.0, |start| ((now - start) / 0.7).clamp(0.0, 1.0) as f32);
    let turn = hover * 0.5 + ease_out_cubic(spun) * std::f32::consts::TAU;

    let painter = ui.painter();
    let text_color = match (kind, enabled) {
        (ButtonKind::Primary, true) => Color32::WHITE,
        (ButtonKind::Glass, true) => TEXT,
        (ButtonKind::Ghost, true) => lerp_color(MUTED, TEXT, hover),
        (ButtonKind::Ghost, false) => MUTED.gamma_multiply(0.5),
        (_, false) => MUTED.gamma_multiply(0.7),
    };
    match kind {
        // White text needs 4.5:1, so the primary fill stays a deep violet even when hovered.
        ButtonKind::Primary => {
            let fill = if !enabled {
                Color32::from_rgb(52, 46, 76)
            } else if pressed {
                Color32::from_rgb(98, 60, 205)
            } else {
                lerp_color(Color32::from_rgb(112, 70, 228), PURPLE, hover)
            };
            painter.rect_filled(rect, radius, fill);
        }
        ButtonKind::Glass => {
            let (fill, edge) = if enabled {
                (
                    (14.0 + 14.0 * hover + if pressed { 6.0 } else { 0.0 }) as u8,
                    (38.0 + 30.0 * hover) as u8,
                )
            } else {
                (8, 20)
            };
            painter.rect_filled(rect, radius, Color32::from_white_alpha(fill));
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(1.0_f32, Color32::from_white_alpha(edge)),
                egui::StrokeKind::Inside,
            );
        }
        ButtonKind::Ghost => {
            if hover > 0.0 {
                painter.rect_filled(rect, radius, Color32::from_white_alpha((14.0 * hover) as u8));
            }
        }
    }

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
            full.expand(2.5),
            egui::CornerRadius::same(r + 3),
            Stroke::new(1.5_f32, PURPLE_BRIGHT),
            egui::StrokeKind::Outside,
        );
    }
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

pub(crate) const FIELD_HEIGHT: f32 = 42.0;
pub(crate) const LIST_ROW: f32 = 38.0;
pub(crate) const LIST_PAD: f32 = 6.0;
pub(crate) const LIST_MAX: f32 = 300.0;

/// The frosted panel behind a dropdown's list. Nearly opaque, so rows stay readable over the sunset.
pub(crate) fn popup_frame() -> Frame {
    Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(22, 15, 46, 244))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(52)))
        .corner_radius(egui::CornerRadius::same(14))
        .inner_margin(Margin::same(6))
        .shadow(egui::epaint::Shadow {
            offset: [0, 14],
            blur: 38,
            spread: 0,
            color: Color32::from_black_alpha(150),
        })
}

/// One row of a dropdown list.
pub(crate) fn dropdown_row(ui: &mut egui::Ui, text: &str, selected: bool, highlighted: bool, interactive: bool) -> egui::Response {
    // CLICK without FOCUSABLE: Tab keeps moving between fields, not through list rows.
    let sense = if interactive { egui::Sense::CLICK } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), LIST_ROW), sense);
    let hl = ui.ctx().animate_bool_with_time(response.id.with("hl"), highlighted, 0.10);
    let painter = ui.painter();
    let inner = rect.shrink2(Vec2::new(0.0, 1.0));
    let radius = egui::CornerRadius::same(9);
    if selected && hl < 1.0 {
        painter.rect_filled(inner, radius, Color32::from_white_alpha(((1.0 - hl) * 12.0) as u8));
    }
    if hl > 0.0 {
        painter.rect_filled(inner, radius, Color32::from_rgba_unmultiplied(139, 91, 249, (hl * 84.0) as u8));
        painter.rect_stroke(
            inner,
            radius,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(214, 190, 255, (hl * 72.0) as u8)),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(inner.left_top() + Vec2::new(0.0, 10.0), Vec2::new(3.0, inner.height() - 20.0)),
            2.0,
            CORAL.gamma_multiply(hl),
        );
    }
    let color = if selected {
        Color32::WHITE
    } else {
        lerp_color(Color32::from_rgb(214, 207, 232), Color32::WHITE, hl)
    };
    let galley = fitted_text(painter, text, 14.5, FontFamily::Proportional, color, inner.width() - 16.0 - 44.0);
    painter.galley(
        egui::pos2(inner.left() + 16.0, inner.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if selected {
        paint_icon(
            painter,
            Icon::Check,
            egui::pos2(inner.right() - 22.0, inner.center().y),
            15.0,
            Color32::from_rgb(255, 164, 128),
            0.0,
        );
    }
    if interactive {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// A dropdown: an inset glass field and a frosted list that eases open. Keyboard: Enter, Space or
/// the arrow keys open it from the field; arrows, Home, End move; Enter or Space pick; Escape
/// closes. Returns true when the pick changed.
pub(crate) fn premium_dropdown(
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
                glass_card(ui, popup_frame(), |ui| {
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

    // The field itself: an inset well, a chevron chip and an open glow.
    let hover = ctx.animate_bool_with_time(id.with("hover"), enabled && response.hovered(), 0.14);
    let painter = ui.painter();
    let radius = egui::CornerRadius::same(12);
    let dim = if enabled { 1.0 } else { 0.5 };
    if t > 0.0 {
        painter.rect_filled(rect.expand(3.0), egui::CornerRadius::same(15), VIOLET.gamma_multiply(0.22 * t));
    }
    painter.rect_filled(rect, radius, Color32::from_rgba_unmultiplied(8, 5, 22, 124));
    painter.rect_filled(rect, radius, Color32::from_white_alpha((10.0 * hover) as u8));
    painter.rect_filled(rect, radius, VIOLET.gamma_multiply(0.16 * t));
    let shade = Color32::from_black_alpha(64);
    paint_gradient(
        painter,
        egui::Rect::from_min_max(
            egui::pos2(rect.left() + 12.0, rect.top() + 1.0),
            egui::pos2(rect.right() - 12.0, rect.top() + 11.0),
        ),
        shade,
        shade,
        Color32::TRANSPARENT,
        Color32::TRANSPARENT,
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(
            1.0_f32,
            lerp_rgba(
                Color32::from_white_alpha(36),
                Color32::from_rgba_unmultiplied(198, 168, 255, 170),
                (hover * 0.55).max(t),
            ),
        ),
        egui::StrokeKind::Inside,
    );
    let lit = hover.max(t);
    let text_color = lerp_color(Color32::from_rgb(226, 220, 242), Color32::WHITE, lit).gamma_multiply(dim);
    let galley = fitted_text(painter, label, 14.5, medium(), text_color, rect.width() - 16.0 - 48.0);
    painter.galley(
        egui::pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    let chip = egui::pos2(rect.right() - 24.0, rect.center().y);
    painter.circle_filled(chip, 12.0, Color32::from_white_alpha((12.0 + 14.0 * hover + 16.0 * t) as u8));
    // The chevron flips through a flat line as the list opens.
    let flip = 1.0 - 2.0 * t;
    painter.add(egui::Shape::line(
        vec![
            chip + Vec2::new(-4.0, -1.8 * flip),
            chip + Vec2::new(0.0, 1.8 * flip),
            chip + Vec2::new(4.0, -1.8 * flip),
        ],
        Stroke::new(1.7_f32, lerp_color(MUTED, TEXT, lit).gamma_multiply(dim)),
    ));
    if response.has_focus() && !open {
        painter.rect_stroke(
            rect.expand(2.5),
            egui::CornerRadius::same(14),
            Stroke::new(1.5_f32, PURPLE_BRIGHT),
            egui::StrokeKind::Outside,
        );
    }
    if enabled && response.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    changed
}

pub(crate) const SLIDER_MAX_WIDTH: f32 = 380.0;

/// A whole-number slider: inset rail with a sunset fill, a glowing thumb and a value chip.
/// The value changes live while dragging. Returns true when a value is final: the pointer was
/// released, or an arrow key was pressed (Shift moves by ten).
pub(crate) fn premium_slider(
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
        let fill_end = lerp_color(VIOLET, CORAL, fraction);
        let painter = ui.painter();
        let rail = egui::Rect::from_min_max(
            egui::pos2(left, area.center().y - 3.0),
            egui::pos2(left + span, area.center().y + 3.0),
        );
        painter.rect_filled(rail, 3.0, Color32::from_rgba_unmultiplied(8, 5, 22, 150));
        painter.rect_stroke(rail, 3.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
        let thumb_x = left + span * fraction;
        let filled = egui::Rect::from_min_max(rail.left_top(), egui::pos2(thumb_x, rail.bottom()));
        let (from, to) = if enabled {
            (VIOLET, fill_end)
        } else {
            (Color32::from_rgb(78, 66, 112), Color32::from_rgb(78, 66, 112))
        };
        if filled.width() > 6.5 {
            paint_round_gradient(painter, filled, 3, from, to, false);
        } else if filled.width() > 0.5 {
            painter.rect_filled(filled, 3.0, from);
        }
        let centre = egui::pos2(thumb_x, area.center().y);
        if enabled {
            painter.circle_filled(centre, 12.0 + 4.0 * hover, VIOLET.gamma_multiply(0.18 + 0.18 * hover));
        }
        painter.circle_filled(centre + Vec2::new(0.0, 1.5), 10.0, Color32::from_black_alpha(70));
        painter.circle_filled(centre, 9.5, if enabled { Color32::WHITE } else { Color32::from_rgb(150, 142, 172) });
        painter.circle_filled(centre, 3.5, if enabled { fill_end } else { Color32::from_rgb(78, 66, 112) });
        if response.has_focus() {
            painter.circle_stroke(centre, 14.5, Stroke::new(1.5_f32, PURPLE_BRIGHT));
        }
        if enabled && (response.hovered() || response.dragged()) {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let (chip, _) = ui.allocate_exact_size(Vec2::new(chip_width, 30.0), egui::Sense::hover());
        let painter = ui.painter();
        painter.rect_filled(chip, 9.0, Color32::from_rgba_unmultiplied(8, 5, 22, 124));
        painter.rect_stroke(chip, 9.0, Stroke::new(1.0_f32, Color32::from_white_alpha(34)), egui::StrokeKind::Inside);
        painter.text(
            chip.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}{}", *value, suffix),
            FontId::new(13.5, medium()),
            if enabled { TEXT } else { MUTED.gamma_multiply(0.6) },
        );
    });
    committed
}

pub(crate) const TRACK_HEIGHT: f32 = 42.0;
pub(crate) const TRACK_INSET: f32 = 4.0;

/// A row of mutually exclusive choices in an inset track, with a thumb that slides to the pick.
/// Returns the value that was clicked, when it differs from `current`.
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
    let track_radius = egui::CornerRadius::same(13);
    painter.rect_filled(track, track_radius, Color32::from_rgba_unmultiplied(8, 5, 22, 124));
    let shade = Color32::from_black_alpha(64);
    paint_gradient(
        painter,
        egui::Rect::from_min_max(
            egui::pos2(track.left() + 12.0, track.top() + 1.0),
            egui::pos2(track.right() - 12.0, track.top() + 11.0),
        ),
        shade,
        shade,
        Color32::TRANSPARENT,
        Color32::TRANSPARENT,
    );
    painter.rect_stroke(track, track_radius, Stroke::new(1.0_f32, Color32::from_white_alpha(30)), egui::StrokeKind::Inside);
    for (response, hover) in responses.iter().zip(&hovers) {
        if *hover > 0.0 {
            painter.rect_filled(
                response.rect,
                egui::CornerRadius::same(9),
                Color32::from_white_alpha((hover * 14.0) as u8),
            );
        }
    }
    if has_pick > 0.0 {
        let thumb = egui::Rect::from_min_size(
            track.left_top() + Vec2::new(TRACK_INSET + segment * slot, TRACK_INSET),
            Vec2::new(segment, TRACK_HEIGHT - 2.0 * TRACK_INSET),
        );
        let fill = if enabled { Color32::from_rgb(112, 70, 228) } else { Color32::from_rgb(70, 58, 108) };
        painter.rect_filled(thumb, egui::CornerRadius::same(9), fill);
    }
    for (i, (_, label)) in options.iter().enumerate() {
        let cell = responses[i].rect;
        // The label brightens as the thumb slides under it.
        let near = (1.0 - (slot - i as f32).abs()).clamp(0.0, 1.0) * has_pick;
        let base = lerp_color(Color32::from_rgb(190, 183, 212), TEXT, hovers[i]);
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
                Stroke::new(1.5_f32, PURPLE_BRIGHT),
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
