//! Palette, typography and shared surface styling.

use eframe::egui::{self, Color32, Context, FontFamily, Frame, Margin, Stroke, Vec2};

pub(crate) const SIDEBAR: Color32 = Color32::from_rgb(17, 15, 34);
pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(9, 7, 25);
pub(crate) const TEXT: Color32 = Color32::from_rgb(246, 242, 255);
pub(crate) const MUTED: Color32 = Color32::from_rgb(204, 198, 224);
pub(crate) const PURPLE: Color32 = Color32::from_rgb(123, 78, 245);
pub(crate) const PURPLE_BRIGHT: Color32 = Color32::from_rgb(214, 190, 255);
pub(crate) const SUCCESS: Color32 = Color32::from_rgb(146, 226, 160);
pub(crate) const DANGER: Color32 = Color32::from_rgb(255, 172, 184);
// Sunset-derived accents shared by the launcher chrome (the backdrop itself lives in scene.rs).
pub(crate) const VIOLET: Color32 = Color32::from_rgb(143, 92, 255);
pub(crate) const CORAL: Color32 = Color32::from_rgb(239, 124, 99);
/// Coral for small text: lighter than `CORAL` so it keeps 4.5:1 contrast over the sunset glow.
pub(crate) const CORAL_TEXT: Color32 = Color32::from_rgb(255, 168, 140);
/// Opacity of the main glass panel. Higher keeps text readable over the bright sunset glow;
/// lower shows more of the sunset. Text colours are chosen for at least 4.5:1 at this value.
pub(crate) const GLASS_ALPHA: u8 = 140;

pub(crate) fn medium() -> FontFamily {
    FontFamily::Name("Horizon Geist Medium".into())
}

pub(crate) fn semibold() -> FontFamily {
    FontFamily::Name("Horizon Geist SemiBold".into())
}

pub(crate) fn configure_egui(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Horizon Geist".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../horizon-source/crates/horizon-theme/fonts/Geist-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Horizon Geist".into());
    // Medium and SemiBold weights: egui has no synthetic bold, so titles need real font files.
    let stack = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (name, family, bytes) in [
        (
            "Horizon Geist Medium",
            medium(),
            &include_bytes!("../horizon-source/crates/horizon-theme/fonts/Geist-Medium.ttf")[..],
        ),
        (
            "Horizon Geist SemiBold",
            semibold(),
            &include_bytes!("../horizon-source/crates/horizon-theme/fonts/Geist-SemiBold.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        let mut with_fallbacks = stack.clone();
        with_fallbacks.insert(0, name.into());
        fonts.families.insert(family, with_fallbacks);
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = Color32::from_rgb(26, 18, 48);
    style.visuals.window_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 40));
    // Separators and hairlines use a faint white so they read as glass edges.
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 26));
    style.visuals.extreme_bg_color = SIDEBAR;
    style.visuals.widgets.noninteractive.fg_stroke.color = TEXT;
    style.visuals.widgets.inactive.fg_stroke.color = TEXT;
    style.visuals.widgets.hovered.fg_stroke.color = TEXT;
    style.visuals.widgets.active.fg_stroke.color = TEXT;
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgba_unmultiplied(115, 69, 176, 65);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(204, 174, 255, 64));
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgba_unmultiplied(146, 96, 224, 110);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(225, 205, 255, 120));
    style.visuals.widgets.active.bg_fill = PURPLE;
    style.visuals.widgets.open.bg_fill = Color32::from_rgb(46, 29, 72);
    // Any stock button that slips through still gets the glass look instead of flat grey.
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgba_unmultiplied(115, 69, 176, 65);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgba_unmultiplied(146, 96, 224, 110);
    style.visuals.widgets.active.weak_bg_fill = PURPLE;
    style.visuals.widgets.open.weak_bg_fill = Color32::from_rgb(46, 29, 72);
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(205, 173, 255, 90));
    style.visuals.popup_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(130) };
    style.visuals.selection.bg_fill = PURPLE;
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.interact_size = Vec2::new(40.0, 36.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
    style.visuals.window_corner_radius = egui::CornerRadius::same(16);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);
    ctx.set_style(style);
}


pub(crate) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// The main translucent panel that holds a page. The sunset remains visible through it.
pub(crate) fn glass_frame(alpha: u8) -> Frame {
    Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(16, 11, 36, alpha))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(26)))
        .corner_radius(egui::CornerRadius::same(20))
        .inner_margin(Margin::symmetric(32, 28))
        .shadow(egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(60),
        })
}

/// A restrained nested surface. `highlight` marks hover or active state without a bright glass wash.
pub(crate) fn inner_card(highlight: bool) -> Frame {
    Frame::NONE
        .fill(if highlight {
            Color32::from_rgba_unmultiplied(52, 36, 82, 218)
        } else {
            Color32::from_rgba_unmultiplied(19, 15, 33, 218)
        })
        .stroke(Stroke::new(
            1.0_f32,
            if highlight {
                Color32::from_rgba_unmultiplied(187, 147, 255, 148)
            } else {
                Color32::from_rgba_unmultiplied(216, 205, 239, 32)
            },
        ))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(Margin::symmetric(18, 14))
}

/// A four-corner gradient quad (top-left, top-right, bottom-right, bottom-left).
pub(crate) fn paint_gradient(painter: &egui::Painter, rect: egui::Rect, tl: Color32, tr: Color32, br: Color32, bl: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), tl);
    mesh.colored_vertex(rect.right_top(), tr);
    mesh.colored_vertex(rect.right_bottom(), br);
    mesh.colored_vertex(rect.left_bottom(), bl);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

