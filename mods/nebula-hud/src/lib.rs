//! Nebula HUD: FPS, clicks per second and position in one dark panel with a violet edge, in the
//! corner the player picks. It gives way to F3's debug screen like Horizon's own boxes.
mod model;

use std::sync::Mutex;

use mod_api::{
    ClientHost, Corner, Flow, Hooks, MouseButtonPreEvent, Registrar, RegistrationContext,
    RenderFramePreEvent, RenderGuiPostEvent, SettingSpec,
};

use model::{Button, ClickCounter, FpsCounter, Show};

/// The mod's id: its settings folder.
pub const ID: &str = "nebula_hud";

const BACKGROUND: u32 = 0xB010_0E1E;
const EDGE: u32 = 0xFF8F_5CFF;
const LABEL: u32 = 0xFFB8_B0D8;
const INK: u32 = 0xFFFF_FFFF;

const CORNERS: [(Corner, &str); 4] = [
    (Corner::TopLeft, "top_left"),
    (Corner::TopRight, "top_right"),
    (Corner::BottomLeft, "bottom_left"),
    (Corner::BottomRight, "bottom_right"),
];

fn settings() -> [SettingSpec; 4] {
    [
        SettingSpec::choice(
            "corner",
            "nebula_hud.setting.corner",
            CORNERS.map(|(_, name)| format!("nebula_hud.corner.{name}")),
            0,
        ),
        SettingSpec::toggle("fps", "nebula_hud.setting.fps", true),
        SettingSpec::toggle("cps", "nebula_hud.setting.cps", true),
        SettingSpec::toggle("coords", "nebula_hud.setting.coords", true),
    ]
}

#[derive(Default)]
struct State {
    fps: FpsCounter,
    clicks: ClickCounter,
}

#[derive(Default)]
struct NebulaHud {
    // Never held across a host call: a host call can fire another of this mod's hooks.
    state: Mutex<State>,
}

impl NebulaHud {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn seconds(host: &dyn ClientHost) -> f64 {
    host.millis() as f64 / 1000.0
}

fn player(host: &dyn ClientHost) -> Option<([f64; 3], f32)> {
    let entity = host.local_player()?.entity();
    let position = entity.position(host)?;
    let [yaw, _] = entity.rotation(host)?;
    Some((position, yaw))
}

#[mod_api::horizon_mod]
impl Hooks for NebulaHud {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for setting in settings() {
            let _ = r.register_setting(setting);
        }
    }

    fn render_frame_pre(&self, _: &mut dyn ClientHost, event: &mut RenderFramePreEvent) {
        self.state().fps.frame(event.elapsed.as_secs_f64());
    }

    fn mouse_button_pre(&self, host: &mut dyn ClientHost, event: &mut MouseButtonPreEvent) -> Flow {
        if let Some(button) = Button::from_glfw(event.button)
            && event.pressed
            && host.playing()
            && host.on(ID)
        {
            let now = seconds(host);
            self.state().clicks.press(button, now);
        }
        Flow::Continue
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        if !host.on(ID) {
            return;
        }
        let corner = CORNERS[host
            .setting_choice("corner")
            .map_or(0, |i| i as usize)
            .min(CORNERS.len() - 1)]
        .0;
        if !host.native_corner_shown(corner) {
            return;
        }
        let show = Show {
            fps: host.setting_bool("fps").unwrap_or(true),
            cps: host.setting_bool("cps").unwrap_or(true),
            coords: host.setting_bool("coords").unwrap_or(true),
        };
        let now = seconds(host);
        let (fps, clicks) = {
            let mut state = self.state();
            (state.fps.fps(), state.clicks.per_second(now))
        };
        let rows = model::rows(show, fps, clicks, player(host));
        if rows.is_empty() {
            return;
        }

        let draw = &mut *event.draw;
        let label_width = rows.iter().map(|r| draw.text_width(r.label)).fold(0.0, f32::max);
        let value_width = rows.iter().map(|r| draw.text_width(&r.value)).fold(0.0, f32::max);
        let [w, h] = model::panel_size(label_width, value_width, rows.len());
        let (x, y) = event.corners.place(corner, w, h);

        // Corners are cut by one pixel: the draw API has rectangles only.
        let edge = model::ACCENT;
        draw.fill([x + edge, y, w - edge - 1.0, 1.0], BACKGROUND);
        draw.fill([x + edge, y + 1.0, w - edge, h - 2.0], BACKGROUND);
        draw.fill([x + edge, y + h - 1.0, w - edge - 1.0, 1.0], BACKGROUND);
        draw.fill([x, y + 1.0, edge, h - 2.0], EDGE);

        let left = x + edge + model::PAD;
        let value_x = left + label_width + model::COLUMN_GAP;
        for (i, row) in rows.iter().enumerate() {
            let top = y + model::PAD + model::ROW * i as f32 + 1.0;
            draw.text(row.label, [left, top], LABEL, true);
            draw.text(&row.value, [value_x, top], INK, true);
        }
    }
}
