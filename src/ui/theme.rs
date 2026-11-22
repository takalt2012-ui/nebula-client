use eframe::egui::{self, Color32, Context, FontFamily, Stroke, Vec2};

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) sidebar: Color32,
    pub(crate) background: Color32,
    pub(crate) surface: Color32,
    pub(crate) surface_raised: Color32,
    pub(crate) border: Color32,
    pub(crate) text: Color32,
    pub(crate) muted: Color32,
    pub(crate) primary: Color32,
    pub(crate) primary_hover: Color32,
    pub(crate) primary_bright: Color32,
    pub(crate) primary_soft: Color32,
    pub(crate) success: Color32,
    pub(crate) danger: Color32,
}

pub(crate) const THEME: Palette = Palette {
    sidebar: Color32::from_rgb(17, 15, 34),
    background: Color32::from_rgb(9, 7, 25),
    surface: Color32::from_rgb(18, 15, 34),
    surface_raised: Color32::from_rgb(27, 22, 44),
    border: Color32::from_rgb(63, 54, 86),
    text: Color32::from_rgb(246, 242, 255),
    muted: Color32::from_rgb(190, 184, 210),
    primary: Color32::from_rgb(123, 78, 245),
    primary_hover: Color32::from_rgb(139, 96, 250),
    primary_bright: Color32::from_rgb(187, 147, 255),
    primary_soft: Color32::from_rgb(39, 29, 64),
    success: Color32::from_rgb(146, 226, 160),
    danger: Color32::from_rgb(255, 172, 184),
};

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
            "../../horizon-source/crates/horizon-theme/fonts/Geist-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Horizon Geist".into());

    let proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (name, family, bytes) in [
        (
            "Horizon Geist Medium",
            medium(),
            &include_bytes!("../../horizon-source/crates/horizon-theme/fonts/Geist-Medium.ttf")[..],
        ),
        (
            "Horizon Geist SemiBold",
            semibold(),
            &include_bytes!("../../horizon-source/crates/horizon-theme/fonts/Geist-SemiBold.ttf")[..],
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        let mut fallback_stack = proportional.clone();
        fallback_stack.insert(0, name.into());
        fonts.families.insert(family, fallback_stack);
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = THEME.background;
    style.visuals.window_fill = THEME.surface_raised;
    style.visuals.window_stroke = Stroke::new(1.0_f32, THEME.border);
    style.visuals.extreme_bg_color = THEME.sidebar;
    style.visuals.widgets.noninteractive.fg_stroke.color = THEME.text;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, THEME.border);
    style.visuals.widgets.inactive.fg_stroke.color = THEME.text;
    style.visuals.widgets.hovered.fg_stroke.color = THEME.text;
    style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.inactive.bg_fill = THEME.surface_raised;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, THEME.border);
    style.visuals.widgets.hovered.bg_fill = THEME.primary_soft;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, THEME.primary.gamma_multiply(0.55));
    style.visuals.widgets.active.bg_fill = THEME.primary;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, THEME.primary_bright.gamma_multiply(0.55));
    style.visuals.widgets.open.bg_fill = THEME.surface_raised;
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, THEME.border);
    style.visuals.widgets.inactive.weak_bg_fill = THEME.surface;
    style.visuals.widgets.hovered.weak_bg_fill = THEME.primary_soft;
    style.visuals.widgets.active.weak_bg_fill = THEME.primary;
    style.visuals.widgets.open.weak_bg_fill = THEME.surface_raised;
    style.visuals.selection.bg_fill = THEME.primary;
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 8],
        blur: 22,
        spread: 0,
        color: Color32::from_black_alpha(115),
    };
    style.spacing.item_spacing = Vec2::new(11.0, 10.0);
    style.spacing.interact_size = Vec2::new(40.0, 36.0);
    style.spacing.button_padding = Vec2::new(12.0, 7.0);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);
    style.visuals.window_corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
    ctx.set_style(style);
}
