//! The Immersive interface: the sunset fills the window, the Nebula wordmark sits in the middle
//! and Play is one large control. Every other page opens over it as a card.

use eframe::egui::{
    self, Align, Align2, Color32, Context, FontId, Frame, Layout, Margin, Painter, Pos2, Rect, RichText, Sense,
    Shape, Stroke, UiBuilder, Vec2,
};

use crate::brand::{arc_offsets, star_vertices, tracked_layout};
use crate::theme::*;
use crate::widgets::*;
use crate::{draw_nav_icon, game_is_running, launch_progress, play_button, window_control, NebulaClient, Page, WindowAction};

const DOCK_HEIGHT: f32 = 64.0;
const PLAY_WIDTH: f32 = 300.0;
const PLAY_HEIGHT: f32 = 56.0;
const EDGE: f32 = 40.0;
const ROUND: f32 = 40.0;
const QUICK_PAGES: [(Page, &str); 6] = [
    (Page::Performance, "Performance"),
    (Page::Graphics, "Graphics"),
    (Page::Customize, "Customize HUD"),
    (Page::Shaders, "Shaders"),
    (Page::Diagnostics, "Diagnostics"),
    (Page::Settings, "Settings"),
];

/// Draws `text` one glyph at a time so the letters can be spaced apart, centred on `center`.
fn tracked_text(painter: &Painter, center: Pos2, text: &str, font: &FontId, color: Color32, tracking: f32) {
    let galleys: Vec<_> = text
        .chars()
        .map(|glyph| painter.layout_no_wrap(glyph.to_string(), font.clone(), color))
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|galley| galley.size().x).collect();
    let (offsets, total) = tracked_layout(&widths, tracking);
    for (galley, offset) in galleys.into_iter().zip(offsets) {
        let at = Pos2::new(center.x - total / 2.0 + offset, center.y - galley.size().y / 2.0);
        painter.galley(at, galley, color);
    }
}

/// Tracked text with a soft shadow, so it stays legible over any part of the sunset.
fn shadowed_text(painter: &Painter, center: Pos2, text: &str, font: &FontId, color: Color32, tracking: f32) {
    tracked_text(painter, center + Vec2::new(0.0, 2.0), text, font, Color32::from_black_alpha(120), tracking);
    tracked_text(painter, center, text, font, color, tracking);
}

/// The Nebula mark: an orbit ring that runs from violet to coral, with a four-point star inside.
fn paint_mark(painter: &Painter, center: Pos2, size: f32) {
    let width = size * 0.07;
    let ring: Vec<Pos2> = arc_offsets(size * 0.42, -0.9, 5.0, 48)
        .into_iter()
        .map(|(x, y)| center + Vec2::new(x, y))
        .collect();
    let segments = ring.len().saturating_sub(1).max(1) as f32;
    for (i, pair) in ring.windows(2).enumerate() {
        let color = lerp_color(pal().violet, pal().coral, i as f32 / segments);
        painter.line_segment([pair[0], pair[1]], Stroke::new(width, color));
    }
    if let (Some(first), Some(last)) = (ring.first(), ring.last()) {
        painter.circle_filled(*first, width / 2.0, pal().violet);
        painter.circle_filled(*last, width / 2.0, pal().coral);
    }
    let tips = star_vertices(size * 0.30, size * 0.07);
    for i in 0..tips.len() {
        let (ax, ay) = tips[i];
        let (bx, by) = tips[(i + 1) % tips.len()];
        painter.add(Shape::convex_polygon(
            vec![center, center + Vec2::new(ax, ay), center + Vec2::new(bx, by)],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }
}

/// A round, dark-glass button with an icon painted by `icon`.
fn round_button(
    ui: &mut egui::Ui,
    active: bool,
    tip: &str,
    icon: impl FnOnce(&Painter, Pos2, Color32),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ROUND), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let painter = ui.painter();
    let fill = if active {
        themed(139, 91, 249, 190)
    } else {
        Color32::from_rgba_unmultiplied(14, 10, 30, (150.0 + 50.0 * hover) as u8)
    };
    let button_rect = rect.shrink(0.5);
    let corner = egui::CornerRadius::same(9);
    painter.rect_filled(button_rect, corner, fill);
    painter.rect_stroke(
        button_rect,
        corner,
        Stroke::new(1.0_f32, Color32::from_white_alpha((34.0 + 60.0 * hover) as u8)),
        egui::StrokeKind::Inside,
    );
    if response.has_focus() {
        painter.rect_stroke(
            button_rect.expand(2.0),
            egui::CornerRadius::same(11),
            Stroke::new(1.5_f32, pal().purple_bright),
            egui::StrokeKind::Outside,
        );
    }
    let ink = lerp_color(pal().muted, pal().text, if active { 1.0 } else { hover });
    icon(painter, rect.center(), ink);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

impl NebulaClient {
    /// One frame of the Immersive interface.
    pub(crate) fn immersive(&mut self, ctx: &Context) {
        if !legacy_look() {
            if let Some(texture) = self.immersive_background.as_ref() {
                let screen = ctx.screen_rect();
                ctx.layer_painter(egui::LayerId::background()).image(
                    texture.id(),
                    screen,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) && self.page != Page::Play {
            self.page = Page::Play;
        }
        egui::TopBottomPanel::top("immersive_top")
            .exact_height(44.0)
            .frame(Frame::NONE.inner_margin(Margin::symmetric(12, 5)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let drag_width = (ui.available_width() - 132.0).max(24.0);
                    let (_, drag) = ui.allocate_exact_size(Vec2::new(drag_width, 34.0), Sense::click_and_drag());
                    if drag.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        window_control(ui, ctx, WindowAction::Close);
                        window_control(ui, ctx, WindowAction::Maximize);
                        window_control(ui, ctx, WindowAction::Minimize);
                    });
                });
            });
        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            let rect = ui.max_rect();
            self.immersive_brand(ui, rect);
            let account_uuid = self.selected_account.and_then(|index| self.accounts.get(index)).map(|account| account.uuid.as_str());
            self.skin_viewer.show(ui, rect, account_uuid);
            self.immersive_account(ui, rect);
            self.immersive_dock(ui, ctx, rect);
        });
        self.immersive_overlay(ctx);
    }

    fn immersive_brand(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter();
        let x = rect.center().x;
        let top = rect.top() + (rect.height() * 0.17).max(36.0);
        paint_mark(painter, Pos2::new(x, top), 64.0);
        let name = FontId::new(46.0, semibold());
        shadowed_text(painter, Pos2::new(x, top + 78.0), "NEBULA", &name, pal().text, 14.0);
        let sub = FontId::new(13.0, semibold());
        shadowed_text(painter, Pos2::new(x, top + 118.0), "CLIENT", &sub, pal().coral_text, 11.0);
    }

    fn immersive_account(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let right = rect.right() - EDGE;
        let area = Rect::from_min_max(Pos2::new(right - 420.0, rect.top() + 8.0), Pos2::new(right, rect.bottom() - 20.0));
        let name = self.selected_account().map(|account| account.name.as_str());
        let can_switch = self.accounts.len() > 1;
        let mut switch = false;
        let mut sign_in = false;
        let mut open = None;
        ui.scope_builder(UiBuilder::new().max_rect(area).layout(Layout::top_down(Align::Max)), |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.allocate_ui_with_layout(Vec2::new(area.width(), 56.0), Layout::right_to_left(Align::Center), |ui| {
                let (avatar, response) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::click());
                let painter = ui.painter();
                let initial = name.and_then(|text| text.chars().next());
                painter.circle_filled(
                    avatar.center(),
                    28.0,
                    if initial.is_some() { pal().violet } else { themed(14, 10, 30, 170) },
                );
                match initial {
                    Some(letter) => {
                        painter.text(
                            avatar.center(),
                            Align2::CENTER_CENTER,
                            letter.to_uppercase().to_string(),
                            FontId::new(22.0, semibold()),
                            Color32::WHITE,
                        );
                    }
                    None => paint_icon(painter, Icon::User, avatar.center(), 22.0, pal().muted, 0.0),
                }
                painter.circle_stroke(avatar.center(), 27.0, Stroke::new(2.0_f32, Color32::from_white_alpha(170)));
                if can_switch {
                    switch = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Switch account").clicked();
                }
                ui.add_space(6.0);
                match name {
                    Some(text) => {
                        ui.label(RichText::new(text).size(16.0).family(semibold()).color(pal().text));
                    }
                    None => {
                        let label = RichText::new("Sign in").size(16.0).family(semibold()).color(pal().text);
                        sign_in = ui.add(egui::Label::new(label).sense(Sense::click())).clicked();
                    }
                }
            });
            for (page, tip) in QUICK_PAGES {
                let active = self.page == page;
                let clicked = round_button(ui, active, tip, |painter, center, color| draw_nav_icon(painter, page, center, color)).clicked();
                if clicked {
                    open = Some(if active { Page::Play } else { page });
                }
            }
        });
        if switch {
            let next = self.selected_account.map_or(0, |index| (index + 1) % self.accounts.len());
            self.selected_account = Some(next);
            self.persist_selection();
        }
        if sign_in {
            self.load_accounts();
        }
        if let Some(page) = open {
            self.go_to(page);
        }
    }

    fn immersive_dock(&mut self, ui: &mut egui::Ui, ctx: &Context, rect: Rect) {
        let bottom = rect.bottom() - EDGE;
        let top = bottom - DOCK_HEIGHT;
        let shade = Rect::from_min_max(Pos2::new(rect.left(), top - 90.0), rect.max);
        paint_gradient(
            ui.painter(),
            shade,
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            Color32::from_black_alpha(130),
            Color32::from_black_alpha(130),
        );
        let running = game_is_running();
        let starting = self.pending_game.is_some();
        let blocker = self.play_blocker();

        let play = Rect::from_center_size(Pos2::new(rect.center().x, top + DOCK_HEIGHT / 2.0), Vec2::new(PLAY_WIDTH, PLAY_HEIGHT));
        let mut launch = false;
        ui.scope_builder(UiBuilder::new().max_rect(play), |ui| {
            launch = play_button(ui, PLAY_WIDTH, blocker.is_none() && !starting, starting);
        });
        if launch {
            self.launch(ctx);
        }

        let left = rect.left() + EDGE;
        let reach = (play.left() - 28.0 - left).max(120.0);
        let (state, color) = if running {
            ("RUNNING", pal().success)
        } else if starting {
            ("STARTING", pal().coral_text)
        } else {
            ("READY", pal().purple_bright)
        };
        let before = self.selected_version;
        ui.scope_builder(
            UiBuilder::new().max_rect(Rect::from_min_size(Pos2::new(left, top), Vec2::new(reach, DOCK_HEIGHT))),
            |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 4.0, color);
                    ui.label(RichText::new(state).size(12.0).family(semibold()).color(pal().text));
                    if let Some(reason) = blocker {
                        ui.label(RichText::new(reason).size(12.0).color(pal().muted));
                    }
                });
                if !self.versions.is_empty() {
                    let choices: Vec<(usize, &str)> = self
                        .versions
                        .iter()
                        .enumerate()
                        .map(|(index, version)| (index, version.label.as_str()))
                        .collect();
                    premium_dropdown(ui, egui::Id::new("immersive_build"), reach.min(240.0), &choices, &mut self.selected_version);
                }
            },
        );
        if before != self.selected_version {
            self.selected_resource = self
                .selected_version()
                .and_then(|version| (!version.resources.is_empty()).then_some(0));
            self.persist_selection();
        }

        let refresh = Rect::from_min_size(
            Pos2::new(rect.right() - EDGE - ROUND, top + (DOCK_HEIGHT - ROUND) / 2.0),
            Vec2::splat(ROUND),
        );
        let mut refreshed = false;
        ui.scope_builder(UiBuilder::new().max_rect(refresh), |ui| {
            refreshed = round_button(ui, false, "Refresh builds", |painter, center, color| {
                paint_icon(painter, Icon::Refresh, center, 20.0, color, 0.0);
            })
            .clicked();
        });
        if refreshed {
            self.refresh();
        }

        let toast = Rect::from_min_max(
            Pos2::new(rect.center().x - 260.0, top - 90.0),
            Pos2::new(rect.center().x + 260.0, top - 12.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(toast).layout(Layout::bottom_up(Align::Center)), |ui| {
            if starting {
                launch_progress(ui, ctx.input(|input| input.time) as f32);
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            self.status_line(ui);
        });
    }

    /// The card over the home screen: the chosen page, or first-run setup when no build exists.
    fn immersive_overlay(&mut self, ctx: &Context) {
        let setup_needed = self.versions.is_empty() || self.setup.is_some();
        if self.page == Page::Play && !setup_needed {
            return;
        }
        let screen = ctx.screen_rect();
        let mut close = false;
        egui::Area::new(egui::Id::new("immersive_scrim"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (area, response) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(area, 0.0, Color32::from_black_alpha(120));
                close = response.clicked() && self.page != Page::Play;
            });
        let width = (screen.width() - 120.0).clamp(360.0, 860.0);
        let height = (screen.height() - 150.0).max(240.0);
        egui::Area::new(egui::Id::new("immersive_card"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(screen.center() + Vec2::new(0.0, 14.0))
            .show(ctx, |ui| {
                glass_card(ui, glass_frame(GLASS_ALPHA.saturating_add(40)), |ui| {
                    ui.set_width(width);
                    if self.page != Page::Play {
                        ui.horizontal(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                close |= premium_button(ui, ButtonKind::Ghost, None, "Close", 0.0).clicked();
                            });
                        });
                    }
                    egui::ScrollArea::vertical().max_height(height).auto_shrink([false, true]).show(ui, |ui| {
                        if self.page == Page::Play {
                            self.setup_card(ui);
                        } else {
                            self.page_body(ui, ctx);
                        }
                    });
                });
            });
        if close {
            self.page = Page::Play;
        }
    }
}
