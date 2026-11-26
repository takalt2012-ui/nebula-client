//! Palette, typography and shared surface styling.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, Context, FontFamily, Frame, Margin, Stroke, Vec2};

/// Every colour the launcher paints with. Two sets exist: the current green theme and the
/// violet and coral look from before, kept as Legacy.
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) sidebar: Color32,
    pub(crate) background: Color32,
    pub(crate) text: Color32,
    pub(crate) muted: Color32,
    pub(crate) purple: Color32,
    pub(crate) purple_bright: Color32,
    pub(crate) success: Color32,
    pub(crate) danger: Color32,
    pub(crate) violet: Color32,
    pub(crate) coral: Color32,
    /// Accent for small text: lighter than `coral` so it keeps 4.5:1 over the sunset glow.
    pub(crate) coral_text: Color32,
}

pub(crate) const GREEN: Palette = Palette {
    sidebar: Color32::from_rgb(11, 43, 38),
    background: Color32::from_rgb(5, 31, 32),
    text: Color32::from_rgb(218, 241, 222),
    muted: Color32::from_rgb(176, 211, 186),
    purple: Color32::from_rgb(35, 83, 71),
    purple_bright: Color32::from_rgb(185, 217, 194),
    success: Color32::from_rgb(146, 226, 160),
    danger: Color32::from_rgb(255, 186, 198),
    violet: Color32::from_rgb(35, 83, 71),
    coral: Color32::from_rgb(142, 182, 155),
    coral_text: Color32::from_rgb(176, 211, 186),
};

pub(crate) const LEGACY: Palette = Palette {
    sidebar: Color32::from_rgb(17, 15, 34),
    background: Color32::from_rgb(9, 7, 25),
    text: Color32::from_rgb(246, 242, 255),
    muted: Color32::from_rgb(204, 198, 224),
    purple: Color32::from_rgb(123, 78, 245),
    purple_bright: Color32::from_rgb(214, 190, 255),
    success: Color32::from_rgb(146, 226, 160),
    danger: Color32::from_rgb(255, 172, 184),
    violet: Color32::from_rgb(143, 92, 255),
    coral: Color32::from_rgb(239, 124, 99),
    coral_text: Color32::from_rgb(255, 168, 140),
};

static LEGACY_LOOK: AtomicBool = AtomicBool::new(false);

/// Switches between the green theme and the Legacy look for everything drawn after this call.
pub(crate) fn set_legacy_look(on: bool) {
    LEGACY_LOOK.store(on, Ordering::Relaxed);
}

pub(crate) fn legacy_look() -> bool {
    LEGACY_LOOK.load(Ordering::Relaxed)
}

/// The palette in use right now.
pub(crate) fn pal() -> Palette {
    if legacy_look() { LEGACY } else { GREEN }
}

/// Maps a colour written for the Legacy look to its green-theme equivalent. Anything not in the
/// table passes through unchanged, so neutral greys and whites stay as they are.
fn green_of(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    match rgb {
        (139, 91, 249) | (143, 92, 255) => (35, 83, 71),
        (115, 69, 176) | (46, 29, 72) => (22, 58, 50),
        (146, 96, 224) => (45, 96, 84),
        (225, 205, 255) | (204, 174, 255) | (205, 173, 255) | (214, 190, 255) => (142, 182, 155),
        (16, 11, 36) | (17, 15, 34) | (26, 18, 48) => (11, 43, 38),
        (10, 7, 24) | (9, 7, 25) => (5, 31, 32),
        other => other,
    }
}

/// A translucent colour from a Legacy-look RGB value, translated for the green theme when that
/// theme is active. Use this where a literal tint was written with `from_rgba_unmultiplied`.
pub(crate) fn themed(r: u8, g: u8, b: u8, alpha: u8) -> Color32 {
    let (r, g, b) = if legacy_look() { (r, g, b) } else { green_of((r, g, b)) };
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// An opaque colour from a Legacy-look RGB value, translated like [`themed`].
pub(crate) fn themed_rgb(r: u8, g: u8, b: u8) -> Color32 {
    let (r, g, b) = if legacy_look() { (r, g, b) } else { green_of((r, g, b)) };
    Color32::from_rgb(r, g, b)
}

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
    style.visuals.panel_fill = pal().background;
    style.visuals.window_fill = themed_rgb(26, 18, 48);
    style.visuals.window_stroke = Stroke::new(1.0_f32, themed(255, 255, 255, 40));
    // Separators and hairlines use a faint white so they read as glass edges.
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, themed(255, 255, 255, 26));
    style.visuals.extreme_bg_color = pal().sidebar;
    style.visuals.widgets.noninteractive.fg_stroke.color = pal().text;
    style.visuals.widgets.inactive.fg_stroke.color = pal().text;
    style.visuals.widgets.hovered.fg_stroke.color = pal().text;
    style.visuals.widgets.active.fg_stroke.color = pal().text;
    style.visuals.widgets.inactive.bg_fill = themed(115, 69, 176, 65);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, themed(204, 174, 255, 64));
    style.visuals.widgets.hovered.bg_fill = themed(146, 96, 224, 110);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, themed(225, 205, 255, 120));
    style.visuals.widgets.active.bg_fill = pal().purple;
    style.visuals.widgets.open.bg_fill = themed_rgb(46, 29, 72);
    // Any stock button that slips through still gets the glass look instead of flat grey.
    style.visuals.widgets.inactive.weak_bg_fill = themed(115, 69, 176, 65);
    style.visuals.widgets.hovered.weak_bg_fill = themed(146, 96, 224, 110);
    style.visuals.widgets.active.weak_bg_fill = pal().purple;
    style.visuals.widgets.open.weak_bg_fill = themed_rgb(46, 29, 72);
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, themed(205, 173, 255, 90));
    style.visuals.popup_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(130) };
    style.visuals.selection.bg_fill = pal().purple;
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
        .fill(themed(16, 11, 36, alpha))
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
            themed(52, 36, 82, 218)
        } else {
            themed(19, 15, 33, 218)
        })
        .stroke(Stroke::new(
            1.0_f32,
            if highlight {
                themed(187, 147, 255, 148)
            } else {
                themed(216, 205, 239, 32)
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

