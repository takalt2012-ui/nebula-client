//! Immersive-only Minecraft skin preview.
//!
//! Network and cache work runs on a worker; rendering only consumes decoded texture handles.

use std::{
    env,
    error::Error,
    fmt,
    fs,
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

use eframe::egui::{self, Color32, Context, Painter, Pos2, Rect, Sense, Shape, Stroke, TextureHandle, TextureOptions, Vec2};
use serde::Deserialize;

#[derive(Debug)]
enum SkinError {
    CurlUnavailable,
    RequestFailed,
    InvalidProfile,
    SkinMissing,
    InvalidTexture,
}

impl fmt::Display for SkinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::CurlUnavailable => "curl is unavailable",
            Self::RequestFailed => "Mojang profile could not be reached",
            Self::InvalidProfile => "profile texture data was invalid",
            Self::SkinMissing => "the account has no public skin",
            Self::InvalidTexture => "skin image could not be decoded",
        };
        f.write_str(message)
    }
}

impl Error for SkinError {}

#[derive(Deserialize)]
struct Profile {
    properties: Vec<ProfileProperty>,
}

#[derive(Deserialize)]
struct ProfileProperty {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct TexturePayload {
    textures: TextureSet,
}

#[derive(Deserialize)]
struct TextureSet {
    skin: Option<TextureLocation>,
    cape: Option<TextureLocation>,
}

#[derive(Deserialize)]
struct TextureLocation {
    url: String,
}

struct SkinBytes {
    skin: Vec<u8>,
    cape: Option<Vec<u8>>,
}

#[derive(Default)]
pub(crate) struct SkinViewer {
    requested_uuid: Option<String>,
    receiver: Option<Receiver<Result<SkinBytes, SkinError>>>,
    skin_texture: Option<TextureHandle>,
    cape_texture: Option<TextureHandle>,
    load_error: Option<String>,
    yaw: f32,
}

impl SkinViewer {
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, screen: Rect, uuid: Option<&str>) {
        if self.requested_uuid.as_deref() != uuid {
            self.requested_uuid = uuid.map(str::to_owned);
            self.skin_texture = None;
            self.cape_texture = None;
            self.load_error = None;
            self.receiver = uuid.map(request_skin);
        }

        let received = self.receiver.as_ref().map(Receiver::try_recv);
        match received {
            Some(Ok(Ok(data))) => {
                self.skin_texture = texture_from_bytes(ui.ctx(), "immersive-player-skin", &data.skin);
                self.cape_texture = data.cape.as_deref().and_then(|bytes| texture_from_bytes(ui.ctx(), "immersive-player-cape", bytes));
                if self.skin_texture.is_none() {
                    self.load_error = Some(SkinError::InvalidTexture.to_string());
                }
                self.receiver = None;
            }
            Some(Ok(Err(error))) => {
                self.load_error = Some(error.to_string());
                self.receiver = None;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.load_error = Some("skin worker stopped unexpectedly".into());
                self.receiver = None;
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }

        let preview = Rect::from_center_size(
            Pos2::new(screen.center().x, screen.center().y + 56.0),
            Vec2::new(240.0, 286.0),
        );
        let response = ui.interact(preview, egui::Id::new("immersive_skin_drag"), Sense::drag())
            .on_hover_text("Hold and drag to rotate your skin");
        if response.dragged() {
            self.yaw = (self.yaw + response.drag_delta().x * 0.012).rem_euclid(std::f32::consts::TAU);
        }

        if let Some(texture) = self.skin_texture.as_ref() {
            draw_player(ui.painter(), preview, texture, self.cape_texture.as_ref(), self.yaw);
            ui.painter().text(
                Pos2::new(preview.center().x, preview.bottom() - 3.0),
                egui::Align2::CENTER_BOTTOM,
                "DRAG TO ROTATE",
                egui::FontId::proportional(10.0),
                Color32::from_white_alpha(175),
            );
        } else {
            let label = if uuid.is_none() {
                "Sign in to preview your skin"
            } else if self.load_error.is_some() {
                "Skin preview unavailable"
            } else {
                "Loading your skin…"
            };
            ui.painter().text(
                preview.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(13.0),
                Color32::from_white_alpha(190),
            );
        }
    }
}

fn request_skin(uuid: &str) -> Receiver<Result<SkinBytes, SkinError>> {
    let owned_uuid = uuid.to_owned();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = fetch_skin(&owned_uuid);
        let _ = sender.send(result);
    });
    receiver
}

fn fetch_skin(uuid: &str) -> Result<SkinBytes, SkinError> {
    let normalized: String = uuid.chars().filter(|character| character.is_ascii_hexdigit()).collect();
    if normalized.len() != 32 {
        return Err(SkinError::InvalidProfile);
    }

    let cache = env::var_os("HOME").map(PathBuf::from)
        .map(|home| home.join(".cache").join("nebula-client").join("skins"));
    if let Some(directory) = cache.as_ref() {
        let skin_path = directory.join(format!("{normalized}.png"));
        let cape_path = directory.join(format!("{normalized}-cape.png"));
        let no_cape_path = directory.join(format!("{normalized}-no-cape"));
        if let Ok(skin) = fs::read(&skin_path) {
            let cape = fs::read(&cape_path).ok();
            if cape.is_some() || no_cape_path.exists() {
                return Ok(SkinBytes { skin, cape });
            }
        }
    }

    let profile_url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{normalized}");
    let profile_bytes = curl_get(&profile_url)?;
    let profile: Profile = serde_json::from_slice(&profile_bytes).map_err(|_| SkinError::InvalidProfile)?;
    let encoded = profile.properties.iter().find(|property| property.name == "textures")
        .map(|property| property.value.as_str()).ok_or(SkinError::InvalidProfile)?;
    let decoded = decode_base64(encoded).ok_or(SkinError::InvalidProfile)?;
    let payload: TexturePayload = serde_json::from_slice(&decoded).map_err(|_| SkinError::InvalidProfile)?;
    let skin_url = payload.textures.skin.map(|texture| texture.url).ok_or(SkinError::SkinMissing)?;
    if !skin_url.starts_with("https://textures.minecraft.net/texture/") {
        return Err(SkinError::InvalidProfile);
    }
    let skin = curl_get(&skin_url)?;
    let cape = match payload.textures.cape.map(|texture| texture.url) {
        Some(url) if url.starts_with("https://textures.minecraft.net/texture/") => Some(curl_get(&url)?),
        _ => None,
    };

    if let Some(directory) = cache {
        if fs::create_dir_all(&directory).is_ok() {
            let skin_path = directory.join(format!("{normalized}.png"));
            let cape_path = directory.join(format!("{normalized}-cape.png"));
            let no_cape_path = directory.join(format!("{normalized}-no-cape"));
            let _ = fs::write(skin_path, &skin);
            if let Some(bytes) = cape.as_ref() {
                let _ = fs::write(cape_path, bytes);
                let _ = fs::remove_file(no_cape_path);
            } else {
                let _ = fs::write(no_cape_path, b"");
                let _ = fs::remove_file(cape_path);
            }
        }
    }
    Ok(SkinBytes { skin, cape })
}

fn curl_get(url: &str) -> Result<Vec<u8>, SkinError> {
    let output = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--max-time", "9", url])
        .output()
        .map_err(|_| SkinError::CurlUnavailable)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(SkinError::RequestFailed)
    }
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
        }
    }
    Some(output)
}

fn texture_from_bytes(ctx: &Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        TextureOptions::NEAREST,
    ))
}

#[derive(Clone, Copy)]
struct UvRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl UvRect {
    const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

#[derive(Clone, Copy)]
struct SkinUv {
    front: UvRect,
    back: UvRect,
    right: UvRect,
    left: UvRect,
    top: UvRect,
    bottom: UvRect,
}

impl SkinUv {
    const fn new(front: UvRect, back: UvRect, right: UvRect, left: UvRect, top: UvRect, bottom: UvRect) -> Self {
        Self { front, back, right, left, top, bottom }
    }
}

fn draw_player(painter: &Painter, rect: Rect, skin: &TextureHandle, cape: Option<&TextureHandle>, yaw: f32) {
    let center = Pos2::new(rect.center().x, rect.center().y - 2.0);
    let scale = (rect.height() / 31.0).clamp(6.0, 8.2);
    if let Some(cape_texture) = cape {
        draw_cape(painter, center, scale, yaw, cape_texture);
    }

    let leg = SkinUv::new(
        UvRect::new(4.0, 20.0, 4.0, 12.0), UvRect::new(12.0, 20.0, 4.0, 12.0),
        UvRect::new(0.0, 20.0, 4.0, 12.0), UvRect::new(8.0, 20.0, 4.0, 12.0),
        UvRect::new(4.0, 16.0, 4.0, 4.0), UvRect::new(8.0, 16.0, 4.0, 4.0),
    );
    let torso = SkinUv::new(
        UvRect::new(20.0, 20.0, 8.0, 12.0), UvRect::new(32.0, 20.0, 8.0, 12.0),
        UvRect::new(16.0, 20.0, 4.0, 12.0), UvRect::new(28.0, 20.0, 4.0, 12.0),
        UvRect::new(20.0, 16.0, 8.0, 4.0), UvRect::new(28.0, 16.0, 8.0, 4.0),
    );
    let arm = SkinUv::new(
        UvRect::new(44.0, 20.0, 4.0, 12.0), UvRect::new(52.0, 20.0, 4.0, 12.0),
        UvRect::new(40.0, 20.0, 4.0, 12.0), UvRect::new(48.0, 20.0, 4.0, 12.0),
        UvRect::new(44.0, 16.0, 4.0, 4.0), UvRect::new(48.0, 16.0, 4.0, 4.0),
    );
    let head = SkinUv::new(
        UvRect::new(8.0, 8.0, 8.0, 8.0), UvRect::new(24.0, 8.0, 8.0, 8.0),
        UvRect::new(0.0, 8.0, 8.0, 8.0), UvRect::new(16.0, 8.0, 8.0, 8.0),
        UvRect::new(8.0, 0.0, 8.0, 8.0), UvRect::new(16.0, 0.0, 8.0, 8.0),
    );
    let leg_layer = SkinUv::new(
        UvRect::new(4.0, 36.0, 4.0, 12.0), UvRect::new(12.0, 36.0, 4.0, 12.0),
        UvRect::new(0.0, 36.0, 4.0, 12.0), UvRect::new(8.0, 36.0, 4.0, 12.0),
        UvRect::new(4.0, 32.0, 4.0, 4.0), UvRect::new(8.0, 32.0, 4.0, 4.0),
    );
    let torso_layer = SkinUv::new(
        UvRect::new(20.0, 36.0, 8.0, 12.0), UvRect::new(32.0, 36.0, 8.0, 12.0),
        UvRect::new(16.0, 36.0, 4.0, 12.0), UvRect::new(28.0, 36.0, 4.0, 12.0),
        UvRect::new(20.0, 32.0, 8.0, 4.0), UvRect::new(28.0, 32.0, 8.0, 4.0),
    );
    let arm_layer = SkinUv::new(
        UvRect::new(44.0, 36.0, 4.0, 12.0), UvRect::new(52.0, 36.0, 4.0, 12.0),
        UvRect::new(40.0, 36.0, 4.0, 12.0), UvRect::new(48.0, 36.0, 4.0, 12.0),
        UvRect::new(44.0, 32.0, 4.0, 4.0), UvRect::new(48.0, 32.0, 4.0, 4.0),
    );
    let head_layer = SkinUv::new(
        UvRect::new(40.0, 8.0, 8.0, 8.0), UvRect::new(56.0, 8.0, 8.0, 8.0),
        UvRect::new(32.0, 8.0, 8.0, 8.0), UvRect::new(48.0, 8.0, 8.0, 8.0),
        UvRect::new(40.0, 0.0, 8.0, 8.0), UvRect::new(48.0, 0.0, 8.0, 8.0),
    );

    draw_box(painter, skin.id(), center, scale, yaw, [-2.0, -5.5, 0.0], [4.0, 12.0, 4.0], leg);
    draw_box(painter, skin.id(), center, scale, yaw, [2.0, -5.5, 0.0], [4.0, 12.0, 4.0], leg);
    draw_box(painter, skin.id(), center, scale, yaw, [-2.0, -5.5, 0.0], [4.18, 12.18, 4.18], leg_layer);
    draw_box(painter, skin.id(), center, scale, yaw, [2.0, -5.5, 0.0], [4.18, 12.18, 4.18], leg_layer);
    draw_box(painter, skin.id(), center, scale, yaw, [0.0, 1.0, 0.0], [8.0, 12.0, 4.0], torso);
    draw_box(painter, skin.id(), center, scale, yaw, [0.0, 1.0, 0.0], [8.18, 12.18, 4.18], torso_layer);
    draw_box(painter, skin.id(), center, scale, yaw, [-6.0, 1.0, 0.0], [4.0, 12.0, 4.0], arm);
    draw_box(painter, skin.id(), center, scale, yaw, [6.0, 1.0, 0.0], [4.0, 12.0, 4.0], arm);
    draw_box(painter, skin.id(), center, scale, yaw, [-6.0, 1.0, 0.0], [4.18, 12.18, 4.18], arm_layer);
    draw_box(painter, skin.id(), center, scale, yaw, [6.0, 1.0, 0.0], [4.18, 12.18, 4.18], arm_layer);
    draw_box(painter, skin.id(), center, scale, yaw, [0.0, 10.5, 0.0], [8.0, 8.0, 8.0], head);
    draw_box(painter, skin.id(), center, scale, yaw, [0.0, 10.5, 0.0], [8.28, 8.28, 8.28], head_layer);
}

fn draw_cape(painter: &Painter, center: Pos2, scale: f32, yaw: f32, texture: &TextureHandle) {
    let corners = [
        project(center, scale, yaw, [-5.0, 7.0, -2.4]),
        project(center, scale, yaw, [5.0, 7.0, -2.4]),
        project(center, scale, yaw, [5.0, -6.0, -2.4]),
        project(center, scale, yaw, [-5.0, -6.0, -2.4]),
    ];
    let uv = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
    paint_quad(painter, texture.id(), corners, uv);
}

fn draw_box(painter: &Painter, texture: egui::TextureId, origin: Pos2, scale: f32, yaw: f32, center: [f32; 3], size: [f32; 3], uv: SkinUv) {
    let [hx, hy, hz] = [size[0] / 2.0, size[1] / 2.0, size[2] / 2.0];
    let point = |x: f32, y: f32, z: f32| project(origin, scale, yaw, [center[0] + x, center[1] + y, center[2] + z]);
    let front = [point(-hx, hy, hz), point(hx, hy, hz), point(hx, -hy, hz), point(-hx, -hy, hz)];
    let back = [point(hx, hy, -hz), point(-hx, hy, -hz), point(-hx, -hy, -hz), point(hx, -hy, -hz)];
    let right = [point(hx, hy, hz), point(hx, hy, -hz), point(hx, -hy, -hz), point(hx, -hy, hz)];
    let left = [point(-hx, hy, -hz), point(-hx, hy, hz), point(-hx, -hy, hz), point(-hx, -hy, -hz)];
    let top = [point(-hx, hy, -hz), point(hx, hy, -hz), point(hx, hy, hz), point(-hx, hy, hz)];
    let face = if yaw.cos() >= 0.0 { (front, uv.front) } else { (back, uv.back) };
    paint_uv_face(painter, texture, face.0, face.1);
    let side = if yaw.sin() >= 0.0 { (right, uv.right) } else { (left, uv.left) };
    paint_uv_face(painter, texture, side.0, side.1);
    paint_uv_face(painter, texture, top, uv.top);
}

fn project(origin: Pos2, scale: f32, yaw: f32, point: [f32; 3]) -> Pos2 {
    let (sin, cos) = yaw.sin_cos();
    let x = point[0] * cos + point[2] * sin;
    let z = -point[0] * sin + point[2] * cos;
    Pos2::new(origin.x + x * scale, origin.y - point[1] * scale + z * scale * 0.28)
}

fn paint_uv_face(painter: &Painter, texture: egui::TextureId, points: [Pos2; 4], uv: UvRect) {
    const SKIN_SIZE: f32 = 64.0;
    let uv_points = [
        Pos2::new(uv.x / SKIN_SIZE, uv.y / SKIN_SIZE),
        Pos2::new((uv.x + uv.w) / SKIN_SIZE, uv.y / SKIN_SIZE),
        Pos2::new((uv.x + uv.w) / SKIN_SIZE, (uv.y + uv.h) / SKIN_SIZE),
        Pos2::new(uv.x / SKIN_SIZE, (uv.y + uv.h) / SKIN_SIZE),
    ];
    paint_quad(painter, texture, points, uv_points);
}

fn paint_quad(painter: &Painter, texture: egui::TextureId, points: [Pos2; 4], uv: [Pos2; 4]) {
    let mut mesh = egui::Mesh::with_texture(texture);
    let base = mesh.vertices.len() as u32;
    for index in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex { pos: points[index], uv: uv[index], color: Color32::WHITE });
    }
    mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    painter.add(Shape::mesh(mesh));
}
