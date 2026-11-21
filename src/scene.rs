//! Pixel sunset backdrop adapted from Horizon Launcher `launcher/src/scene.rs`.
//! Copyright (c) 2026 Ludovic Marie and the Horizon contributors; MIT License.
//! This is a small egui port of its palette, seeded star field, layered ridges and sun.

use eframe::egui::{self, Color32, Context, Vec2};
use std::f64::consts::PI;

const SKY: [u32; 15] = [
    0x0a0919, 0x0f0c28, 0x151136, 0x1d1444, 0x281850, 0x361b5b, 0x481e66, 0x5d236e,
    0x752873, 0x902f76, 0xab3a75, 0xc64a71, 0xde5f6a, 0xef7c63, 0xf89d62,
];
const SUN: [u32; 7] = [0xfff5d2, 0xffe4a0, 0xffca78, 0xffac5f, 0xff8c58, 0xfb6c62, 0xee5277];
const RIDGE_COLORS: [u32; 3] = [0x3f1f5c, 0x25123e, 0x100a1f];
const RIDGE_RIMS: [u32; 3] = [0xc45a7f, 0x7a3068, 0x2c1640];
const TREES: [&[&str]; 3] = [
    &["..###..", ".#####.", "#######", "#######", ".#####.", "...#...", "...#..."],
    &["..#..", "..#..", ".###.", "..#..", ".###.", "#####", "..#..", "..#.."],
    &[".###.", "#####", "#####", "..#..", "..#.."],
];
const PINK_HAZE: u32 = 0xe0697c;

#[derive(Clone, Copy)]
struct Star {
    x: f32,
    y: f32,
    size: f32,
    brightness: f32,
    phase: f32,
    speed: f32,
    warm: bool,
}

#[derive(Default)]
pub struct Sunset {
    size: [u32; 2],
    pixel: f32,
    horizon: f32,
    sun: [f32; 3],
    stars: Vec<Star>,
    ridges: [Vec<i32>; 3],
    trees: Vec<(usize, usize)>,
}

impl Sunset {
    pub fn paint(&mut self, ctx: &Context) {
        let rect = ctx.screen_rect();
        let size = [rect.width().round().max(1.0) as u32, rect.height().round().max(1.0) as u32];
        if self.size != size {
            self.prepare(size);
        }
        let painter = ctx.layer_painter(egui::LayerId::background());
        let w = rect.width();
        let h = rect.height();
        let time = ctx.input(|i| i.time) as f32;
        let pointer = ctx.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
        let lean = ((pointer.x / w.max(1.0)) - 0.5).clamp(-0.5, 0.5) * self.pixel * 1.4;

        // Horizon's original 15-stop palette and curved vertical interpolation.
        const BANDS: usize = 112;
        const COLUMNS: usize = 36;
        for row in 0..BANDS {
            let y = h * (row as f32 + 0.5) / BANDS as f32;
            let y0 = rect.top() + h * row as f32 / BANDS as f32;
            let y1 = rect.top() + h * (row + 1) as f32 / BANDS as f32 + 1.0;
            for col in 0..COLUMNS {
                let x0 = rect.left() + w * col as f32 / COLUMNS as f32;
                let x1 = rect.left() + w * (col + 1) as f32 / COLUMNS as f32 + 1.0;
                let x = w * (col as f32 + 0.5) / COLUMNS as f32;
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)),
                    0.0,
                    self.sky_color(x, y),
                );
            }
        }

        // Small pixel clouds from Horizon's cloud palette; their parallax is deliberately restrained.
        for layer in 0..3 {
            let speed = [0.5, 1.0, 2.0][layer];
            let y = rect.top() + h * [0.39, 0.53, 0.65][layer];
            let span = w * 1.4;
            let x = rect.left() + ((time * speed * 5.0 + layer as f32 * w * 0.29) % span) - w * 0.2 - lean * (layer as f32 + 1.0);
            let cw = w * [0.13, 0.21, 0.28][layer];
            let ch = self.pixel * [1.7, 1.3, 1.0][layer];
            let cloud = Color32::from_rgba_unmultiplied(120, 64, 125, [24, 29, 34][layer]);
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(cw, ch)), 0.0, cloud);
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(x + cw * 0.18, y - ch), Vec2::new(cw * 0.55, ch)), 0.0, cloud);
            painter.rect_filled(egui::Rect::from_min_size(egui::pos2(x + cw * 0.63, y - ch * 0.5), Vec2::new(cw * 0.32, ch * 0.5)), 0.0, cloud);
        }

        // The wide-screen sun: a soft halo and the source launcher's sliced, slowly drifting disc.
        let center = egui::pos2(rect.left() + self.sun[0] + lean, rect.top() + self.sun[1]);
        let radius = self.sun[2];
        for ring in (1..=10).rev() {
            let r = radius * (1.0 + ring as f32 * 0.17);
            let alpha = (2.5 + (11 - ring) as f32 * 0.8) as u8;
            painter.circle_filled(center, r, Color32::from_rgba_unmultiplied(255, 114, 106, alpha));
        }
        let stripe_h = (radius / 20.0).max(3.0);
        let mut y = -radius;
        while y < radius {
            let dy = y + stripe_h * 0.5;
            let half = (radius * radius - dy * dy).max(0.0).sqrt();
            let t = ((dy / radius + 1.0) * 0.5).clamp(0.0, 1.0);
            let color = palette_mix(&SUN, t);
            let stripe_phase = ((time * 2.2 * self.pixel) % (stripe_h * 5.0)).abs();
            let gap = t > 0.36 && (y + radius + stripe_phase) % (stripe_h * 5.0) < stripe_h * (0.55 + t * 0.9);
            if !gap {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(center.x - half, center.y + y),
                        egui::pos2(center.x + half, center.y + y + stripe_h + 0.5),
                    ),
                    0.0,
                    color,
                );
            }
            y += stripe_h;
        }

        // Deterministically seeded stars, twinkling in stepped brightness levels like the original.
        for (i, star) in self.stars.iter().enumerate() {
            let phase = ((time / (4.0 + (i % 4) as f32 * 2.0) + star.phase / (PI as f32 * 2.0)) % 1.0) * 4.0;
            let level = match phase as usize { 0 => 0.95, 1 => 0.6, 2 => 0.3, _ => 0.6 };
            let alpha = (star.brightness * level * 255.0) as u8;
            let c = if star.warm { [255, 243, 228] } else { [210, 200, 255] };
            let x = rect.left() + (star.x + time * star.speed * 0.04 + lean * 0.35).rem_euclid(w.max(1.0));
            painter.circle_filled(egui::pos2(x, rect.top() + star.y), star.size, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], alpha));
        }

        // The source's three noise-built ridges are drawn back to front in snapped 4px columns.
        for layer in 0..3 {
            let tops = &self.ridges[layer];
            let color = color32(RIDGE_COLORS[layer]);
            let rim = RIDGE_RIMS[layer];
            for (col, &top) in tops.iter().enumerate() {
                let x = rect.left() + (col as f32 - 20.0) * self.pixel + lean * (0.2 + layer as f32 * 0.35);
                if x > rect.right() || x + self.pixel < rect.left() { continue; }
                let y = rect.top() + self.horizon + top as f32 * self.pixel;
                let above = col > 0 && top < tops[col - 1];
                let below = col + 1 < tops.len() && top < tops[col + 1];
                let lit = ((col as f32 * self.pixel - self.sun[0]).abs() / (w * 0.3)).powi(2);
                let ridge_color = if above || below {
                    blend(color, color32(rim), (0.72 * (-lit).exp()).clamp(0.0, 1.0))
                } else { color };
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(x, y), egui::pos2(x + self.pixel + 0.5, rect.bottom())),
                    0.0,
                    ridge_color,
                );
            }
            if layer == 2 {
                for &(col, sprite) in &self.trees {
                    let base = tops[col] as f32;
                    for (row, mask) in TREES[sprite].iter().enumerate() {
                        for (dx, pixel) in mask.bytes().enumerate() {
                            if pixel == b'#' {
                                let x = rect.left() + (col as f32 - 20.0 + dx as f32 - 3.0) * self.pixel + lean;
                                let y = rect.top() + self.horizon + (base - (TREES[sprite].len() - row) as f32) * self.pixel;
                                painter.rect_filled(egui::Rect::from_min_size(egui::pos2(x, y), Vec2::splat(self.pixel)), 0.0, color);
                            }
                        }
                    }
                }
            }
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(31));
    }

    fn prepare(&mut self, size: [u32; 2]) {
        self.size = size;
        let (width, height) = (size[0] as f32, size[1] as f32);
        self.pixel = ((width / 340.0).round()).clamp(3.0, 6.0);
        let land = (height * 0.3).clamp(180.0, 320.0);
        let h_tex = (height / self.pixel).ceil();
        let land_tex = land / self.pixel;
        self.horizon = (h_tex - land_tex * 0.55).round() * self.pixel;
        let radius = (land_tex * 0.4 * 1.1).round() * self.pixel;
        self.sun = [width * 0.72, self.horizon - radius * (0.45 + 0.1), radius];
        let count = (width * self.horizon / 5200.0).round() as usize;
        let mut rng = Rng(7);
        self.stars.clear();
        self.stars.reserve(count);
        for _ in 0..count {
            let x = rng.next() as f32 * width;
            let y = 2.0 + (rng.next().powf(1.8) as f32) * self.horizon * 0.75;
            let size = if rng.next() < 0.15 { 1.8 } else { 1.0 + rng.next() as f32 * 0.4 };
            let brightness = if rng.next() < 0.1 { 0.8 + rng.next() as f32 * 0.2 } else { 0.25 + rng.next() as f32 * 0.4 };
            self.stars.push(Star { x, y, size, brightness, phase: rng.next() as f32 * PI as f32 * 2.0, speed: 0.5 + rng.next() as f32 * 2.0, warm: rng.next() >= 0.35 });
        }
        let cols = (width / self.pixel).ceil() as usize + 40;
        let configs = [
            (11, 0.0_f32, 0.3, 2, 170.0, 0.2),
            (23, 0.22, 0.36, 3, 130.0, 0.35),
            (37, 0.5, 0.38, 4, 100.0, 0.5),
        ];
        for (layer, &(seed, base_offset, amp_share, block, feature, valley)) in configs.iter().enumerate() {
            let noise = Noise::new(seed);
            let horizon_tex = self.horizon / self.pixel;
            let mut tops = vec![0_i32; cols];
            let mut i = 0_usize;
            while i < cols {
                let x = (i as f32 - 20.0) + block as f32 / 2.0;
                let d = (x - width * 0.72 / self.pixel).abs() / ((width / self.pixel) * 0.5);
                let env = valley + (1.0 - valley) * (d * d * 1.8).min(1.0);
                let v = ((noise.at(x as f64 * self.pixel as f64 / feature as f64) as f32 - 0.2) / 0.6).clamp(0.0, 1.0);
                let base = horizon_tex + land_tex * base_offset;
                let amp = land_tex * amp_share;
                let top = ((base - amp * env * v) / block as f32).round() as i32 * block;
                for slot in i..(i + block as usize).min(cols) { tops[slot] = top - horizon_tex as i32; }
                i += block as usize;
            }
            self.ridges[layer] = tops;
        }
        self.trees.clear();
        let near_cols = self.ridges[2].len();
        let mut col = 12_usize;
        let mut n = 0_usize;
        while col < near_cols.saturating_sub(8) {
            self.trees.push((col, n % TREES.len()));
            col += 23 + (n * 17 % 24);
            n += 1;
        }
    }

    fn sky_color(&self, x: f32, y: f32) -> Color32 {
        let t = (y / self.horizon.max(1.0)).clamp(0.0, 1.0);
        let f = t.powf(2.3) * 10.0;
        let mut c = color32(rgb_mix(SKY[f.floor() as usize], SKY[(f.floor() as usize + 1).min(14)], f.fract()));
        let ry = self.horizon * 0.32;
        let dx = (x - self.sun[0]) * ry / (self.size[0] as f32 * 0.6);
        let d = ((dx * dx + (y - self.horizon).powi(2)).sqrt() / ry.max(1.0)).clamp(0.0, 1.0);
        let glow = if d < 0.3 { (1.0 - d / 0.3) * 0.55 } else if d < 0.65 { (0.65 - d) / 0.35 * 0.37 } else { 0.0 };
        c = blend(c, color32(SKY[13]), glow);
        if y > self.horizon * 0.82 { c = blend(c, color32(PINK_HAZE), ((y / self.horizon - 0.82) * 0.2).clamp(0.0, 0.3)); }
        c
    }
}

struct Rng(i32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let s = self.0;
        let mut t = (s ^ ((s as u32) >> 15) as i32).wrapping_mul(1 | s);
        t = t.wrapping_add((t ^ ((t as u32) >> 7) as i32).wrapping_mul(61 | t)) ^ t;
        ((t ^ ((t as u32) >> 14) as i32) as u32) as f64 / 4_294_967_296.0
    }
}

struct Noise([f32; 256]);
impl Noise {
    fn new(seed: i32) -> Self { let mut r = Rng(seed); Self(std::array::from_fn(|_| r.next() as f32)) }
    fn at(&self, x: f64) -> f64 {
        let (mut value, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
        for _ in 0..4 {
            let p = x * freq; let i = p.floor() as i64; let s = p - i as f64;
            let a = self.0[(i & 255) as usize] as f64;
            let b = self.0[((i + 1) & 255) as usize] as f64;
            value += (a + (b - a) * s * s * (3.0 - 2.0 * s)) * amp;
            norm += amp; amp *= 0.5; freq *= 2.1;
        }
        value / norm
    }
}

fn color32(c: u32) -> Color32 { Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8) }
fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}
fn rgb_mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| ((((a >> shift) & 255) as f32 + (((b >> shift) & 255) as f32 - ((a >> shift) & 255) as f32) * t) as u32) & 255;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}
fn palette_mix(palette: &[u32], t: f32) -> Color32 {
    let f = t.clamp(0.0, 1.0) * (palette.len() - 1) as f32;
    color32(rgb_mix(palette[f.floor() as usize], palette[(f.floor() as usize + 1).min(palette.len() - 1)], f.fract()))
}
