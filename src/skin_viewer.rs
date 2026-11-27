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

use eframe::egui::{self, Color32, Context, Painter, Pos2, Rect, Sense, Shape, TextureHandle, TextureOptions, Vec2};
use serde::Deserialize;

#[derive(Debug)]
enum SkinError {
    CurlUnavailable,
    RequestFailed,
    InvalidProfile,
    NoMojangProfile,
    SkinMissing,
    InvalidTexture,
}

impl fmt::Display for SkinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::CurlUnavailable => "curl is unavailable",
            Self::RequestFailed => "Mojang profile could not be reached",
            Self::NoMojangProfile => "this account has no Mojang profile (offline accounts have no skin)",
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
    // Mojang writes these keys in upper case; serde is case-sensitive.
    #[serde(rename = "SKIN")]
    skin: Option<TextureLocation>,
    #[serde(rename = "CAPE")]
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
    /// The account is fine but has no custom skin, so Mojang serves the default one.
    default_skin: bool,
    yaw: f32,
}

impl SkinViewer {
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, screen: Rect, uuid: Option<&str>) {
        if self.requested_uuid.as_deref() != uuid {
            self.requested_uuid = uuid.map(str::to_owned);
            self.skin_texture = None;
            self.cape_texture = None;
            self.load_error = None;
            self.default_skin = false;
            self.receiver = uuid.map(request_skin);
        }

        let received = self.receiver.as_ref().map(|receiver| receiver.try_recv());
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
                self.default_skin = matches!(error, SkinError::SkinMissing);
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
            Pos2::new(screen.left() + screen.width() * 0.27, screen.center().y + 12.0),
            Vec2::new(190.0, 250.0),
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
            } else if self.default_skin {
                "Default skin"
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
            if self.default_skin {
                ui.painter().text(
                    preview.center() + Vec2::new(0.0, 22.0),
                    egui::Align2::CENTER_CENTER,
                    "This account has no custom skin yet.",
                    egui::FontId::proportional(11.0),
                    Color32::from_white_alpha(150),
                );
            } else if let Some(reason) = self.load_error.as_deref() {
                ui.painter().text(
                    preview.center() + Vec2::new(0.0, 22.0),
                    egui::Align2::CENTER_CENTER,
                    reason,
                    egui::FontId::proportional(11.0),
                    Color32::from_white_alpha(150),
                );
                let retry = Rect::from_center_size(preview.center() + Vec2::new(0.0, 56.0), Vec2::new(84.0, 28.0));
                let response = ui.interact(retry, egui::Id::new("immersive_skin_retry"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let fill = if response.hovered() { 70 } else { 40 };
                ui.painter().rect_filled(retry, 6.0, Color32::from_white_alpha(fill));
                ui.painter().text(
                    retry.center(),
                    egui::Align2::CENTER_CENTER,
                    "Try again",
                    egui::FontId::proportional(12.0),
                    Color32::WHITE,
                );
                if response.clicked() {
                    self.requested_uuid = None;
                }
            }
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
    if profile_bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(SkinError::NoMojangProfile);
    }
    let profile: Profile = serde_json::from_slice(&profile_bytes).map_err(|_| SkinError::InvalidProfile)?;
    let encoded = profile.properties.iter().find(|property| property.name == "textures")
        .map(|property| property.value.as_str()).ok_or(SkinError::InvalidProfile)?;
    let decoded = decode_base64(encoded).ok_or(SkinError::InvalidProfile)?;
    let payload: TexturePayload = serde_json::from_slice(&decoded).map_err(|_| SkinError::InvalidProfile)?;
    // Mojang lists texture links as http://; the files are served over https too.
    let secure = |url: String| url.replacen("http://", "https://", 1);
    let skin_url = payload.textures.skin.map(|texture| secure(texture.url)).ok_or(SkinError::SkinMissing)?;
    if !skin_url.starts_with("https://textures.minecraft.net/texture/") {
        return Err(SkinError::InvalidProfile);
    }
    let skin = curl_get(&skin_url)?;
    let cape = match payload.textures.cape.map(|texture| secure(texture.url)) {
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
    // Preserve the PNG alpha channel. Transparent outer-layer texels must
    // reveal the base skin underneath; never flatten this texture.
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

#[derive(Clone, Copy)]
struct RenderFace {
    depth: f32,
    points: [Pos2; 4],
    uv: UvRect,
}

fn draw_player(painter: &Painter, rect: Rect, skin: &TextureHandle, cape: Option<&TextureHandle>, yaw: f32) {
    let center = Pos2::new(rect.center().x, rect.center().y - 2.0);
    // Keep the preview comfortably sized inside its panel instead of letting it
    // dominate the launcher. The slight three-quarter camera angle gives the
    // model visible depth even before the user drags it.
    let scale = (rect.height() / 43.0).clamp(4.2, 6.4);
    if let Some(cape_texture) = cape {
        draw_cape(painter, center, scale, yaw, cape_texture);
    }

    let leg = SkinUv::new(
        UvRect::new(4.0, 20.0, 4.0, 12.0), UvRect::new(12.0, 20.0, 4.0, 12.0),
        UvRect::new(0.0, 20.0, 4.0, 12.0), UvRect::new(8.0, 20.0, 4.0, 12.0),
        UvRect::new(4.0, 16.0, 4.0, 4.0), UvRect::new(8.0, 16.0, 4.0, 4.0),
    );
    // The left leg has its own UV island in modern 64x64 skins. Reusing the
    // right-leg island made the second leg and its transparent overlay wrong.
    let left_leg = SkinUv::new(
        UvRect::new(20.0, 52.0, 4.0, 12.0), UvRect::new(28.0, 52.0, 4.0, 12.0),
        UvRect::new(16.0, 52.0, 4.0, 12.0), UvRect::new(24.0, 52.0, 4.0, 12.0),
        UvRect::new(20.0, 48.0, 4.0, 4.0), UvRect::new(24.0, 48.0, 4.0, 4.0),
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
    let left_arm = SkinUv::new(
        UvRect::new(36.0, 52.0, 4.0, 12.0), UvRect::new(44.0, 52.0, 4.0, 12.0),
        UvRect::new(32.0, 52.0, 4.0, 12.0), UvRect::new(40.0, 52.0, 4.0, 12.0),
        UvRect::new(36.0, 48.0, 4.0, 4.0), UvRect::new(40.0, 48.0, 4.0, 4.0),
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
    let left_leg_layer = SkinUv::new(
        UvRect::new(20.0, 36.0, 4.0, 12.0), UvRect::new(28.0, 36.0, 4.0, 12.0),
        UvRect::new(16.0, 36.0, 4.0, 12.0), UvRect::new(24.0, 36.0, 4.0, 12.0),
        UvRect::new(20.0, 32.0, 4.0, 4.0), UvRect::new(24.0, 32.0, 4.0, 4.0),
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
    let left_arm_layer = SkinUv::new(
        UvRect::new(52.0, 52.0, 4.0, 12.0), UvRect::new(60.0, 52.0, 4.0, 12.0),
        UvRect::new(48.0, 52.0, 4.0, 12.0), UvRect::new(56.0, 52.0, 4.0, 12.0),
        UvRect::new(52.0, 48.0, 4.0, 4.0), UvRect::new(56.0, 48.0, 4.0, 4.0),
    );
    let head_layer = SkinUv::new(
        UvRect::new(40.0, 8.0, 8.0, 8.0), UvRect::new(56.0, 8.0, 8.0, 8.0),
        UvRect::new(32.0, 8.0, 8.0, 8.0), UvRect::new(48.0, 8.0, 8.0, 8.0),
        UvRect::new(40.0, 0.0, 8.0, 8.0), UvRect::new(48.0, 0.0, 8.0, 8.0),
    );

    let mut faces = Vec::with_capacity(72);
    add_box_faces(&mut faces, center, scale, yaw, [-2.0, -5.5, 0.0], [4.0, 12.0, 4.0], leg);
    add_box_faces(&mut faces, center, scale, yaw, [2.0, -5.5, 0.0], [4.0, 12.0, 4.0], left_leg);
    add_box_faces(&mut faces, center, scale, yaw, [-2.0, -5.5, 0.0], [4.18, 12.18, 4.18], leg_layer);
    add_box_faces(&mut faces, center, scale, yaw, [2.0, -5.5, 0.0], [4.18, 12.18, 4.18], left_leg_layer);
    add_box_faces(&mut faces, center, scale, yaw, [0.0, 1.0, 0.0], [8.0, 12.0, 4.0], torso);
    add_box_faces(&mut faces, center, scale, yaw, [0.0, 1.0, 0.0], [8.18, 12.18, 4.18], torso_layer);
    add_box_faces(&mut faces, center, scale, yaw, [-6.0, 1.0, 0.0], [4.0, 12.0, 4.0], arm);
    add_box_faces(&mut faces, center, scale, yaw, [6.0, 1.0, 0.0], [4.0, 12.0, 4.0], left_arm);
    add_box_faces(&mut faces, center, scale, yaw, [-6.0, 1.0, 0.0], [4.18, 12.18, 4.18], arm_layer);
    add_box_faces(&mut faces, center, scale, yaw, [6.0, 1.0, 0.0], [4.18, 12.18, 4.18], left_arm_layer);
    add_box_faces(&mut faces, center, scale, yaw, [0.0, 10.5, 0.0], [8.0, 8.0, 8.0], head);
    add_box_faces(&mut faces, center, scale, yaw, [0.0, 10.5, 0.0], [8.28, 8.28, 8.28], head_layer);

    // Sort all cuboid faces together by camera depth. This lets the far arm/leg
    // sit behind the torso while the near-side geometry naturally occludes it.
    faces.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    for face in faces {
        paint_uv_face(painter, skin.id(), face.points, face.uv);
    }
}

fn add_box_faces(
    faces: &mut Vec<RenderFace>,
    origin: Pos2,
    scale: f32,
    yaw: f32,
    center: [f32; 3],
    size: [f32; 3],
    uv: SkinUv,
) {
    let [hx, hy, hz] = [size[0] / 2.0, size[1] / 2.0, size[2] / 2.0];
    let vertices = [
        [-hx, hy, hz], [hx, hy, hz], [hx, -hy, hz], [-hx, -hy, hz],
        [hx, hy, -hz], [-hx, hy, -hz], [-hx, -hy, -hz], [hx, -hy, -hz],
    ];
    let projected = vertices.map(|point| {
        project(origin, scale, yaw, [center[0] + point[0], center[1] + point[1], center[2] + point[2]])
    });
    let camera_yaw = yaw + 0.48;
    let (sin, cos) = camera_yaw.sin_cos();
    let depth = |indices: [usize; 4]| {
        indices.iter().map(|index| {
            let point = vertices[*index];
            -(center[0] + point[0]) * sin + (center[2] + point[2]) * cos
        }).sum::<f32>() / 4.0
    };
    let face_defs = [
        ([0, 1, 2, 3], uv.front),
        ([4, 5, 6, 7], uv.back),
        ([1, 4, 7, 2], uv.right),
        ([5, 0, 3, 6], uv.left),
        ([5, 4, 1, 0], uv.top),
        ([3, 2, 7, 6], uv.bottom),
    ];
    for (indices, face_uv) in face_defs {
        faces.push(RenderFace {
            depth: depth(indices),
            points: indices.map(|index| projected[index]),
            uv: face_uv,
        });
    }
}

fn project(origin: Pos2, scale: f32, yaw: f32, point: [f32; 3]) -> Pos2 {
    let (sin, cos) = (yaw + 0.48).sin_cos();
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
