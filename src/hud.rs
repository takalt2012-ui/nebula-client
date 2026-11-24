//! The Basic HUD native mod's widget configuration: reading it, editing it and writing it back in
//! the mod's own `key=value;` format. Standard library only.

/// Number of widgets the mod draws.
pub(crate) const WIDGET_COUNT: usize = 14;
/// Config version the mod writes; older versions make it migrate positions it believes untouched.
const CONFIG_VERSION: &str = "4";

/// An RGB colour as three bytes.
pub(crate) type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Widget {
    Effects,
    Items,
    Durability,
    Totems,
    Health,
    Cooldowns,
    Coordinates,
    Fps,
    Ping,
    Speed,
    Cps,
    Clock,
    ArmorMini,
    Pickups,
}

impl Widget {
    pub(crate) const ALL: [Self; WIDGET_COUNT] = [
        Self::Fps,
        Self::Cps,
        Self::Coordinates,
        Self::Ping,
        Self::Speed,
        Self::Clock,
        Self::ArmorMini,
        Self::Effects,
        Self::Items,
        Self::Durability,
        Self::Totems,
        Self::Health,
        Self::Cooldowns,
        Self::Pickups,
    ];

    /// Position in [`Self::ALL`], which is also the order the styles are stored in.
    fn index(self) -> usize {
        match self {
            Self::Fps => 0,
            Self::Cps => 1,
            Self::Coordinates => 2,
            Self::Ping => 3,
            Self::Speed => 4,
            Self::Clock => 5,
            Self::ArmorMini => 6,
            Self::Effects => 7,
            Self::Items => 8,
            Self::Durability => 9,
            Self::Totems => 10,
            Self::Health => 11,
            Self::Cooldowns => 12,
            Self::Pickups => 13,
        }
    }

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Effects => "effects",
            Self::Items => "items",
            Self::Durability => "durability",
            Self::Totems => "totems",
            Self::Health => "health",
            Self::Cooldowns => "cooldowns",
            Self::Coordinates => "coordinates",
            Self::Fps => "fps",
            Self::Ping => "ping",
            Self::Speed => "speed",
            Self::Cps => "cps",
            Self::Clock => "clock",
            Self::ArmorMini => "armor_mini",
            Self::Pickups => "pickups",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|widget| widget.key() == key)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Effects => "Effects",
            Self::Items => "Consumables",
            Self::Durability => "Durability",
            Self::Totems => "Totems",
            Self::Health => "Health",
            Self::Cooldowns => "Cooldowns",
            Self::Coordinates => "Coordinates",
            Self::Fps => "FPS",
            Self::Ping => "Ping",
            Self::Speed => "Speed",
            Self::Cps => "CPS",
            Self::Clock => "Clock",
            Self::ArmorMini => "Armor",
            Self::Pickups => "Pickups",
        }
    }

    /// A label and value the preview shows for this widget.
    pub(crate) fn sample(self) -> (&'static str, &'static str) {
        match self {
            Self::Fps => ("FPS", "144"),
            Self::Cps => ("CPS", "6 | 3"),
            Self::Coordinates => ("XYZ", "9 / 76 / -3"),
            Self::Ping => ("Ping", "18 ms"),
            Self::Speed => ("Speed", "4.3 bps"),
            Self::Clock => ("Time", "12:30"),
            Self::ArmorMini => ("Armor", "4 / 4"),
            Self::Effects => ("Speed II", "0:42"),
            Self::Items => ("Gapple", "x3"),
            Self::Durability => ("Sword", "1561"),
            Self::Totems => ("Totems", "2"),
            Self::Health => ("Health", "20"),
            Self::Cooldowns => ("Pearl", "0:12"),
            Self::Pickups => ("+ Torch", "x6"),
        }
    }

    /// Roughly how big the widget is in GUI pixels at scale 1, for the layout preview.
    pub(crate) fn preview_size(self) -> [f32; 2] {
        match self {
            Self::Fps => [54.0, 16.0],
            Self::Cps => [62.0, 16.0],
            Self::Coordinates => [116.0, 28.0],
            Self::Ping => [74.0, 16.0],
            Self::Speed => [64.0, 16.0],
            Self::Clock => [70.0, 16.0],
            Self::ArmorMini => [74.0, 18.0],
            Self::Effects => [92.0, 30.0],
            Self::Items => [84.0, 24.0],
            Self::Durability => [84.0, 30.0],
            Self::Totems => [64.0, 18.0],
            Self::Health => [84.0, 16.0],
            Self::Cooldowns => [92.0, 18.0],
            Self::Pickups => [84.0, 30.0],
        }
    }

    /// The mod's own default for this widget, top-left corner layout.
    pub(crate) fn default_style(self) -> Style {
        let (enabled, x, y) = match self {
            Self::Effects => (true, 0.01, 0.26),
            Self::Items => (true, 0.01, 0.40),
            Self::Durability => (true, 0.01, 0.63),
            Self::Totems => (true, 0.01, 0.86),
            Self::Health => (true, 0.50, 0.72),
            Self::Cooldowns => (true, 0.50, 0.80),
            Self::Coordinates => (true, 0.01, 0.12),
            Self::Fps => (true, 0.01, 0.02),
            Self::Ping => (true, 0.99, 0.02),
            Self::Speed => (false, 0.99, 0.07),
            Self::Cps => (false, 0.01, 0.75),
            Self::Clock => (false, 0.50, 0.02),
            Self::ArmorMini => (true, 0.55, 0.90),
            Self::Pickups => (true, 0.99, 0.40),
        };
        Style::new(enabled, x, y)
    }
}

/// One widget's placement and look. `x` and `y` run 0..=1 across the room left after the widget's
/// own size, so 0.99 hugs the right edge whatever the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Style {
    pub(crate) enabled: bool,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) scale: f32,
    /// None lets the mod colour the text by status.
    pub(crate) text: Option<Rgb>,
    pub(crate) accent: Rgb,
    pub(crate) background: Rgb,
    /// Background opacity in percent.
    pub(crate) opacity: u8,
    pub(crate) shadow: bool,
    pub(crate) padding: f32,
    pub(crate) compact: bool,
    pub(crate) icons: bool,
    pub(crate) icons_only: bool,
}

impl Style {
    pub(crate) const MIN_SCALE: f32 = 0.5;
    pub(crate) const MAX_SCALE: f32 = 3.0;
    pub(crate) const MAX_PADDING: f32 = 12.0;

    fn new(enabled: bool, x: f32, y: f32) -> Self {
        Self {
            enabled,
            x,
            y,
            scale: 1.0,
            text: None,
            accent: [0x70, 0xC9, 0xFF],
            background: [0x10, 0x15, 0x1D],
            opacity: 60,
            shadow: true,
            padding: 3.0,
            compact: false,
            icons: true,
            icons_only: false,
        }
    }

    /// Reads the mod's twelve or thirteen comma-separated fields; anything malformed keeps the
    /// value from `fallback`, and a record of the wrong length is ignored whole.
    fn parse(record: &str, fallback: &Self) -> Self {
        let parts: Vec<&str> = record.split(',').collect();
        if parts.len() != 12 && parts.len() != 13 {
            return *fallback;
        }
        let number = |i: usize, low: f32, high: f32, or: f32| {
            parts[i]
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .map_or(or, |v| v.clamp(low, high))
        };
        let colour = |i: usize, or: Rgb| parse_rgb(parts[i]).unwrap_or(or);
        Self {
            enabled: parts[0] != "0",
            x: number(1, 0.0, 1.0, fallback.x),
            y: number(2, 0.0, 1.0, fallback.y),
            scale: number(3, Self::MIN_SCALE, Self::MAX_SCALE, fallback.scale),
            text: if parts[4].eq_ignore_ascii_case("auto") {
                None
            } else {
                parse_rgb(parts[4]).or(fallback.text)
            },
            accent: colour(5, fallback.accent),
            background: colour(6, fallback.background),
            opacity: parts[7].parse::<u8>().map_or(fallback.opacity, |v| v.min(100)),
            shadow: parts[8] != "0",
            padding: number(9, 0.0, Self::MAX_PADDING, fallback.padding),
            compact: parts[10] != "0",
            icons: parts[11] != "0",
            icons_only: parts.get(12).is_some_and(|v| *v != "0"),
        }
    }

    fn record(&self) -> String {
        let text = self.text.map_or_else(|| "auto".to_owned(), hex);
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            u8::from(self.enabled),
            self.x,
            self.y,
            self.scale,
            text,
            hex(self.accent),
            hex(self.background),
            self.opacity,
            u8::from(self.shadow),
            self.padding,
            u8::from(self.compact),
            u8::from(self.icons),
            u8::from(self.icons_only),
        )
    }
}

fn parse_rgb(text: &str) -> Option<Rgb> {
    let value = u32::from_str_radix(text, 16).ok()?;
    let [_, r, g, b] = value.to_be_bytes();
    Some([r, g, b])
}

fn hex([r, g, b]: Rgb) -> String {
    format!("{r:02X}{g:02X}{b:02X}")
}

/// A ready-made look for the whole HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Preset {
    Lunar,
    Minimal,
    Classic,
}

impl Preset {
    pub(crate) const ALL: [Self; 3] = [Self::Lunar, Self::Minimal, Self::Classic];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Lunar => "Lunar",
            Self::Minimal => "Minimal",
            Self::Classic => "Classic",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::Lunar => "Dark translucent panels stacked top left and top right",
            Self::Minimal => "Just FPS and coordinates, light and quiet",
            Self::Classic => "The Basic HUD's own defaults",
        }
    }
}

/// The mod's config: widget styles we edit, and every other pair kept as it was read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HudConfig {
    others: Vec<(String, String)>,
    styles: [Style; WIDGET_COUNT],
}

impl Default for HudConfig {
    fn default() -> Self {
        Self { others: Vec::new(), styles: Widget::ALL.map(Widget::default_style) }
    }
}

impl HudConfig {
    pub(crate) fn parse(stored: &str) -> Self {
        let mut config = Self::default();
        let mut explicit = [false; WIDGET_COUNT];
        for pair in stored.split(';') {
            let Some((key, value)) = pair.trim().split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if let Some(widget) = key.strip_prefix("w.").and_then(Widget::from_key) {
                let slot = widget.index();
                config.styles[slot] = Style::parse(value, &widget.default_style());
                explicit[slot] = true;
            } else if !key.is_empty() && key != "v" {
                config.others.push((key.to_owned(), value.to_owned()));
            }
        }
        for widget in [Widget::Effects, Widget::Items, Widget::Durability, Widget::Totems] {
            let slot = widget.index();
            let legacy = config.others.iter().find(|(key, _)| key == widget.key());
            if let (false, Some((_, value))) = (explicit[slot], legacy) {
                config.styles[slot].enabled = value != "0";
            }
        }
        config
    }

    pub(crate) fn style(&self, widget: Widget) -> &Style {
        &self.styles[widget.index()]
    }

    pub(crate) fn style_mut(&mut self, widget: Widget) -> &mut Style {
        &mut self.styles[widget.index()]
    }

    pub(crate) fn serialize(&self) -> String {
        let mut parts = Vec::with_capacity(1 + self.others.len() + WIDGET_COUNT);
        parts.push(format!("v={CONFIG_VERSION}"));
        parts.extend(self.others.iter().map(|(key, value)| format!("{key}={value}")));
        parts.extend(
            Widget::ALL
                .iter()
                .map(|widget| format!("w.{}={}", widget.key(), self.style(*widget).record())),
        );
        parts.join(";")
    }

    pub(crate) fn apply(&mut self, preset: Preset) {
        self.styles = Widget::ALL.map(Widget::default_style);
        match preset {
            Preset::Classic => {}
            Preset::Lunar => {
                for style in &mut self.styles {
                    style.text = Some([255, 255, 255]);
                    style.accent = [0x8F, 0x5C, 0xFF];
                    style.background = [0x0E, 0x0D, 0x18];
                    style.opacity = 55;
                    style.padding = 4.0;
                }
                let places = [
                    (Widget::Fps, 0.01, 0.02, true),
                    (Widget::Cps, 0.01, 0.11, true),
                    (Widget::Coordinates, 0.01, 0.20, true),
                    (Widget::Ping, 0.99, 0.02, true),
                    (Widget::Speed, 0.99, 0.11, false),
                    (Widget::Clock, 0.99, 0.20, false),
                    (Widget::Effects, 0.99, 0.30, true),
                    (Widget::Items, 0.99, 0.50, true),
                    (Widget::Pickups, 0.99, 0.72, true),
                ];
                for (widget, x, y, enabled) in places {
                    let style = self.style_mut(widget);
                    (style.x, style.y, style.enabled) = (x, y, enabled);
                }
            }
            Preset::Minimal => {
                for style in &mut self.styles {
                    style.text = Some([255, 255, 255]);
                    style.background = [0x00, 0x00, 0x00];
                    style.opacity = 30;
                    style.shadow = true;
                    style.padding = 2.0;
                }
                for widget in [Widget::Cps, Widget::Ping, Widget::Speed, Widget::Clock] {
                    self.style_mut(widget).enabled = false;
                }
                self.style_mut(Widget::Coordinates).y = 0.10;
            }
        }
    }
}

/// Where a widget of `size` GUI pixels sits in a `viewport` of GUI pixels: its top-left corner.
pub(crate) fn place(style: &Style, size: [f32; 2], viewport: [f32; 2]) -> [f32; 2] {
    let scale = style.scale.clamp(Style::MIN_SCALE, Style::MAX_SCALE);
    let free = |axis: usize| (viewport[axis] - size[axis] * scale).max(0.0);
    [style.x.clamp(0.0, 1.0) * free(0), style.y.clamp(0.0, 1.0) * free(1)]
}

/// Inverse of [`place`]: the `x`/`y` that put the widget's top-left at `top_left`.
pub(crate) fn normalise(style: &Style, size: [f32; 2], viewport: [f32; 2], top_left: [f32; 2]) -> [f32; 2] {
    let scale = style.scale.clamp(Style::MIN_SCALE, Style::MAX_SCALE);
    let axis = |i: usize| {
        let free = (viewport[i] - size[i] * scale).max(0.0);
        if free <= f32::EPSILON { 0.0 } else { (top_left[i] / free).clamp(0.0, 1.0) }
    };
    [axis(0), axis(1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOD_RECORD: &str = "1,0.01,0.02,1,auto,70C9FF,10151D,60,1,3,0,1,0";

    #[test]
    fn a_record_the_mod_wrote_round_trips() {
        let style = Style::parse(MOD_RECORD, &Widget::Fps.default_style());
        assert_eq!(style, Widget::Fps.default_style());
        assert_eq!(style.record(), MOD_RECORD);
    }

    #[test]
    fn short_records_and_bad_fields_fall_back() {
        let fallback = Widget::Fps.default_style();
        assert_eq!(Style::parse("1,0.5", &fallback), fallback);
        let odd = Style::parse("1,nope,9,99,zz,70C9FF,10151D,500,1,3,0,1,0", &fallback);
        assert_eq!(odd.x, fallback.x);
        assert_eq!(odd.y, 1.0);
        assert_eq!(odd.scale, Style::MAX_SCALE);
        assert_eq!(odd.text, fallback.text);
        assert_eq!(odd.opacity, fallback.opacity);
    }

    #[test]
    fn unknown_pairs_survive_an_edit() {
        let mut config = HudConfig::parse("v=2;language=de_de;pinned=a,b;w.fps=1,0.3,0.4,1,auto,70C9FF,10151D,60,1,3,0,1,0");
        config.style_mut(Widget::Fps).enabled = false;
        let written = config.serialize();
        assert!(written.starts_with("v=4;language=de_de;pinned=a,b;"));
        let again = HudConfig::parse(&written);
        assert!(!again.style(Widget::Fps).enabled);
        assert_eq!(again.style(Widget::Fps).x, 0.3);
        assert_eq!(again, config);
    }

    #[test]
    fn a_widget_missing_from_the_file_keeps_the_mods_default_place() {
        let config = HudConfig::parse("v=4;language=auto");
        assert_eq!(config.style(Widget::Coordinates).y, 0.12);
        assert_eq!(config.style(Widget::Ping).x, 0.99);
    }

    #[test]
    fn legacy_switches_apply_only_without_a_widget_record() {
        let config = HudConfig::parse("effects=0;items=0;w.items=1,0.01,0.4,1,auto,70C9FF,10151D,60,1,3,0,1,0");
        assert!(!config.style(Widget::Effects).enabled);
        assert!(config.style(Widget::Items).enabled);
    }

    #[test]
    fn every_widget_is_written_once() {
        let written = HudConfig::default().serialize();
        for widget in Widget::ALL {
            let needle = format!("w.{}=", widget.key());
            assert_eq!(written.matches(&needle).count(), 1, "{}", widget.key());
        }
    }

    #[test]
    fn presets_never_stack_two_widgets_on_one_spot() {
        for preset in Preset::ALL {
            let mut config = HudConfig::default();
            config.apply(preset);
            let on: Vec<_> = Widget::ALL.into_iter().filter(|w| config.style(*w).enabled).collect();
            for (i, a) in on.iter().enumerate() {
                for b in &on[i + 1..] {
                    let (sa, sb) = (config.style(*a), config.style(*b));
                    let viewport = [427.0, 240.0];
                    let (pa, pb) = (place(sa, a.preview_size(), viewport), place(sb, b.preview_size(), viewport));
                    let (za, zb) = (a.preview_size(), b.preview_size());
                    let overlap = pa[0] < pb[0] + zb[0] && pb[0] < pa[0] + za[0] && pa[1] < pb[1] + zb[1] && pb[1] < pa[1] + za[1];
                    assert!(!overlap, "{preset:?}: {} overlaps {}", a.label(), b.label());
                }
            }
        }
    }

    #[test]
    fn placing_and_normalising_are_inverses() {
        let mut style = Widget::Fps.default_style();
        style.x = 0.37;
        style.y = 0.61;
        style.scale = 1.5;
        let viewport = [427.0, 240.0];
        let size = Widget::Fps.preview_size();
        let top_left = place(&style, size, viewport);
        let [x, y] = normalise(&style, size, viewport, top_left);
        assert!((x - 0.37).abs() < 1e-4 && (y - 0.61).abs() < 1e-4);
    }
}
