//! Pure state and arithmetic behind the panel. Nothing here touches the game, so it is unit tested
//! with the standard library alone.

use std::collections::VecDeque;

/// Seconds between two FPS readings; a steadier number is easier to read than every frame's.
const FPS_REFRESH: f64 = 0.5;
/// More clicks than any hand makes in a second; the queues never grow past it.
const CLICKS_KEPT: usize = 64;

/// Frames per second over the last second, recomputed twice a second.
#[derive(Debug, Default)]
pub struct FpsCounter {
    now: f64,
    ends: VecDeque<f64>,
    shown: u32,
    computed_at: f64,
}

impl FpsCounter {
    /// Records a frame that took `seconds`.
    pub fn frame(&mut self, seconds: f64) {
        self.now += seconds.max(0.0);
        self.ends.push_back(self.now);
        while self.ends.front().is_some_and(|&t| self.now - t >= 1.0) {
            self.ends.pop_front();
        }
        if self.now - self.computed_at >= FPS_REFRESH {
            self.computed_at = self.now;
            self.shown = u32::try_from(self.ends.len()).unwrap_or(u32::MAX);
        }
    }

    pub fn fps(&self) -> u32 {
        self.shown
    }
}

/// Which mouse button a click came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
}

impl Button {
    /// GLFW numbers the left button 0 and the right 1; any other button is not counted.
    pub fn from_glfw(button: i32) -> Option<Self> {
        match button {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            _ => None,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

/// Clicks of the left and right buttons in the last second.
#[derive(Debug, Default)]
pub struct ClickCounter {
    times: [VecDeque<f64>; 2],
}

impl ClickCounter {
    pub fn press(&mut self, button: Button, now: f64) {
        let queue = &mut self.times[button.index()];
        if queue.len() == CLICKS_KEPT {
            queue.pop_front();
        }
        queue.push_back(now);
    }

    /// `(left, right)` clicks in the second up to `now`.
    pub fn per_second(&mut self, now: f64) -> (u32, u32) {
        let [left, right] = self.times.each_mut().map(|queue| {
            while queue.front().is_some_and(|&t| now - t >= 1.0) {
                queue.pop_front();
            }
            u32::try_from(queue.len()).unwrap_or(u32::MAX)
        });
        (left, right)
    }
}

/// What the player turned on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Show {
    pub fps: bool,
    pub cps: bool,
    pub coords: bool,
}

/// One line of the panel: a dim label and a bright value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
}

/// The rows to draw, in a fixed order. Position rows need a player, so they are left out outside
/// a world.
pub fn rows(show: Show, fps: u32, clicks: (u32, u32), player: Option<([f64; 3], f32)>) -> Vec<Row> {
    let mut rows = Vec::with_capacity(4);
    if show.fps {
        rows.push(Row { label: "FPS", value: fps.to_string() });
    }
    if show.cps {
        rows.push(Row { label: "CPS", value: format!("{} | {}", clicks.0, clicks.1) });
    }
    if let (true, Some(([x, y, z], yaw))) = (show.coords, player) {
        rows.push(Row { label: "XYZ", value: format!("{} / {} / {}", block(x), block(y), block(z)) });
        rows.push(Row { label: "DIR", value: compass(yaw).to_owned() });
    }
    rows
}

/// A coordinate as the block it is in; `+ 0` keeps a floored `-0` from printing a sign.
fn block(v: f64) -> String {
    format!("{}", (v.floor() + 0.0) as i64)
}

/// Direction the player faces: yaw 0 is south, 90 west.
pub fn compass(yaw: f32) -> &'static str {
    let i = (yaw / 90.0 + 0.5).floor() as i32 & 3;
    ["South", "West", "North", "East"][i as usize]
}

/// Size of the panel in GUI pixels for columns `label_width` and `value_width` wide.
pub fn panel_size(label_width: f32, value_width: f32, rows: usize) -> [f32; 2] {
    [
        ACCENT + 2.0 * PAD + label_width + COLUMN_GAP + value_width,
        2.0 * PAD + ROW * rows as f32,
    ]
}

/// Width of the coloured edge strip.
pub const ACCENT: f32 = 2.0;
/// Space between the panel's edge and its text.
pub const PAD: f32 = 4.0;
/// Height of one row.
pub const ROW: f32 = 11.0;
/// Space between the label column and the value column.
pub const COLUMN_GAP: f32 = 6.0;

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: Show = Show { fps: true, cps: true, coords: true };

    #[test]
    fn fps_counts_the_frames_of_the_last_second() {
        let mut counter = FpsCounter::default();
        for _ in 0..300 {
            counter.frame(1.0 / 100.0);
        }
        assert!((99..=101).contains(&counter.fps()), "{}", counter.fps());
    }

    #[test]
    fn clicks_age_out_after_a_second() {
        let mut clicks = ClickCounter::default();
        clicks.press(Button::Left, 10.0);
        clicks.press(Button::Left, 10.5);
        clicks.press(Button::Right, 10.9);
        assert_eq!(clicks.per_second(10.95), (2, 1));
        assert_eq!(clicks.per_second(11.2), (1, 1));
        assert_eq!(clicks.per_second(12.0), (0, 0));
    }

    #[test]
    fn click_queues_stay_bounded() {
        let mut clicks = ClickCounter::default();
        for i in 0..1000 {
            clicks.press(Button::Left, 5.0 + i as f64 * 1e-6);
        }
        assert_eq!(clicks.per_second(5.01).0 as usize, CLICKS_KEPT);
    }

    #[test]
    fn only_left_and_right_buttons_count() {
        assert_eq!(Button::from_glfw(0), Some(Button::Left));
        assert_eq!(Button::from_glfw(1), Some(Button::Right));
        assert_eq!(Button::from_glfw(2), None);
    }

    #[test]
    fn rows_follow_the_switches() {
        let off = Show { fps: false, cps: false, coords: false };
        assert!(rows(off, 60, (0, 0), Some(([0.0; 3], 0.0))).is_empty());
        let labels: Vec<_> = rows(ALL, 144, (6, 3), Some(([1.5, 70.0, -3.2], 180.0))).iter().map(|r| r.label).collect();
        assert_eq!(labels, ["FPS", "CPS", "XYZ", "DIR"]);
    }

    #[test]
    fn position_rows_wait_for_a_player() {
        let labels: Vec<_> = rows(ALL, 60, (0, 0), None).iter().map(|r| r.label).collect();
        assert_eq!(labels, ["FPS", "CPS"]);
    }

    #[test]
    fn coordinates_are_the_block_the_player_is_in() {
        let r = rows(ALL, 60, (0, 0), Some(([-0.4, 64.9, 12.0], 0.0)));
        assert_eq!(r[2].value, "-1 / 64 / 12");
        let r = rows(ALL, 60, (0, 0), Some(([-0.0, 70.0, 0.0], 0.0)));
        assert_eq!(r[2].value, "0 / 70 / 0");
    }

    #[test]
    fn compass_matches_the_games_yaw() {
        assert_eq!(compass(0.0), "South");
        assert_eq!(compass(90.0), "West");
        assert_eq!(compass(180.0), "North");
        assert_eq!(compass(-90.0), "East");
        assert_eq!(compass(-44.0), "South");
    }

    #[test]
    fn panel_grows_with_its_rows() {
        let [w, h] = panel_size(16.0, 40.0, 3);
        assert_eq!(h, 2.0 * PAD + 3.0 * ROW);
        assert_eq!(w, ACCENT + 2.0 * PAD + 16.0 + COLUMN_GAP + 40.0);
    }
}
