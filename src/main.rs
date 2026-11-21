//! A deliberately small independent interface for an installed Horizon build.
//! It delegates managed game starts to Horizon's own `play` command so cache leases,
//! instance configuration and account-session handoff remain correct.

mod scene;

use eframe::egui::{
    self, Align, Button, Color32, ComboBox, Context, FontFamily, FontId, Frame, Layout, Margin,
    RichText, Stroke, Vec2,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Instant, SystemTime};

const SIDEBAR: Color32 = Color32::from_rgb(17, 15, 34);
const BACKGROUND: Color32 = Color32::from_rgb(9, 7, 25);
const TEXT: Color32 = Color32::from_rgb(246, 242, 255);
const MUTED: Color32 = Color32::from_rgb(173, 166, 194);
const PURPLE: Color32 = Color32::from_rgb(123, 78, 245);
const PURPLE_BRIGHT: Color32 = Color32::from_rgb(187, 147, 255);
const SUCCESS: Color32 = Color32::from_rgb(146, 226, 160);
const DANGER: Color32 = Color32::from_rgb(255, 156, 171);
// Sunset-derived accents shared by the launcher chrome (the backdrop itself lives in scene.rs).
const VIOLET: Color32 = Color32::from_rgb(143, 92, 255);
const CORAL: Color32 = Color32::from_rgb(239, 124, 99);
/// Opacity of the main glass panel. Lower shows more sunset, higher improves text contrast.
const GLASS_ALPHA: u8 = 118;
const HUD_WIDGETS: [&str; 14] = ["effects", "items", "durability", "totems", "health", "cooldowns", "coordinates", "fps", "ping", "speed", "cps", "clock", "armor_mini", "pickups"];
const HUD_FEATURE_LABELS: [(&str, &str); 14] = [
    ("Effects", "Potion and status effects"), ("Items", "Consumable timers"),
    ("Durability", "Equipment durability"), ("Totems", "Totems used"),
    ("Health", "Player health tags"), ("Cooldowns", "Item cooldowns"),
    ("Coordinates", "Position and facing"), ("FPS", "Frame rate"),
    ("Ping", "Network latency"), ("Speed", "Movement speed"),
    ("CPS", "Clicks per second"), ("Clock", "In-game time"),
    ("Armor", "Compact armor HUD"), ("Recent pickups", "Recent item pickups"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Play,
    Performance,
    Shaders,
    Diagnostics,
    Settings,
}

impl Page {
    const ALL: [(Self, &'static str); 5] = [
        (Self::Play, "Home"),
        (Self::Performance, "Performance"),
        (Self::Shaders, "Shaders"),
        (Self::Diagnostics, "Diagnostics"),
        (Self::Settings, "Settings"),
    ];
}

#[derive(Clone)]
struct HorizonVersion {
    label: String,
    root: PathBuf,
    client: PathBuf,
    resources: Vec<ResourceSet>,
    modified: SystemTime,
}

#[derive(Clone)]
struct ResourceSet {
    label: String,
    path: PathBuf,
}

#[derive(Clone, Deserialize)]
struct AccountInfo {
    name: String,
    uuid: String,
    selected: bool,
}

#[derive(Deserialize)]
struct AccountList {
    accounts: Vec<AccountInfo>,
}

#[derive(Clone, Copy)]
struct Capabilities {
    gamemode: bool,
    mangohud: bool,
    ionice: bool,
}

impl Capabilities {
    fn detect() -> Self {
        Self {
            gamemode: command_exists("gamemoderun"),
            mangohud: command_exists("mangohud"),
            ionice: command_exists("ionice"),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct LauncherSettings {
    performance_mode: bool,
    gamemode: bool,
    mangohud: bool,
    io_priority: bool,
    close_after_launch: bool,
    opengl_threading: bool,
    nvidia_threading: bool,
    selected_version: Option<String>,
    selected_resource: Option<String>,
    selected_account: Option<String>,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            performance_mode: true,
            gamemode: false,
            mangohud: false,
            io_priority: true,
            close_after_launch: true,
            opengl_threading: false,
            nvidia_threading: false,
            selected_version: None,
            selected_resource: None,
            selected_account: None,
        }
    }
}

struct Status {
    message: String,
    error: bool,
}

#[derive(Clone)]
struct ShaderPackEntry {
    /// File or folder name inside `shaderpacks/`.
    file: String,
    /// Value written to `pickedShaderPack:`.
    key: String,
    title: String,
    /// A Horizon-native pack (`horizon-shaders.toml` / `.hshader`), else classic `shaders/`.
    native: bool,
}

struct PendingGame {
    child: Child,
    started: Instant,
}

struct NebulaClient {
    sunset: scene::Sunset,
    logo: egui::TextureHandle,
    page: Page,
    settings: LauncherSettings,
    capabilities: Capabilities,
    versions: Vec<HorizonVersion>,
    accounts: Vec<AccountInfo>,
    official_launcher: Option<PathBuf>,
    selected_version: Option<usize>,
    selected_resource: Option<usize>,
    selected_account: Option<usize>,
    status: Option<Status>,
    pending_game: Option<PendingGame>,
    hud_config_path: Option<PathBuf>,
    hud_features: [bool; 14],
    shader_packs: Vec<ShaderPackEntry>,
    shader_picked: String,
    shader_dir: Option<PathBuf>,
    shader_graphics: Option<PathBuf>,
}

impl NebulaClient {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_egui(&cc.egui_ctx);
        let logo = load_horizon_logo(&cc.egui_ctx);
        let mut app = Self {
            sunset: scene::Sunset::default(),
            logo,
            page: Page::Play,
            settings: load_settings().unwrap_or_default(),
            capabilities: Capabilities::detect(),
            versions: Vec::new(),
            accounts: Vec::new(),
            official_launcher: discover_official_launcher(),
            selected_version: None,
            selected_resource: None,
            selected_account: None,
            status: None,
            pending_game: None,
            hud_config_path: None,
            hud_features: [false; 14],
            shader_packs: Vec::new(),
            shader_picked: String::new(),
            shader_dir: None,
            shader_graphics: None,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.versions = discover_versions();
        self.capabilities = Capabilities::detect();
        self.official_launcher = discover_official_launcher();

        self.selected_version = find_path_index(
            &self.versions,
            self.settings.selected_version.as_deref(),
            |version| &version.root,
        )
        .or_else(|| {
            self.versions
                .iter()
                .position(|version| version.label.starts_with("native-"))
        })
        .or_else(|| (!self.versions.is_empty()).then_some(0));
        self.selected_resource = self.selected_version.and_then(|index| {
            find_path_index(
                &self.versions[index].resources,
                self.settings.selected_resource.as_deref(),
                |resource| &resource.path,
            )
            .or_else(|| (!self.versions[index].resources.is_empty()).then_some(0))
        });
        self.selected_account = self
            .accounts
            .iter()
            .position(|account| {
                self.settings.selected_account.as_deref() == Some(account.uuid.as_str())
            })
            .or_else(|| self.accounts.iter().position(|account| account.selected))
            .or_else(|| (!self.accounts.is_empty()).then_some(0));
        self.persist_selection();
        self.load_hud_features();
        self.load_shader_state();
    }

    fn load_hud_features(&mut self) {
        self.hud_config_path = None;
        self.hud_features = [false; 14];
        let (Some(version), Some(launcher)) = (self.selected_version(), self.official_launcher.as_deref()) else { return; };
        let Some(tag) = version.root.file_name().and_then(|name| name.to_str()) else { return; };
        let Ok(instance) = selected_instance(launcher, tag) else { return; };
        let instance_root = horizon_data_root().join("horizon/instances").join(instance);
        if !instance_root.join("horizon-native-mods/basic_hud.hnmod").is_file() { return; }
        let path = instance_root.join("horizon-mods/.horizon-mod-data/basic_hud/config");
        self.hud_config_path = Some(path.clone());
        self.hud_features = [true, true, true, true, true, true, true, true, true, false, false, false, true, true];
        if let Ok(config) = fs::read_to_string(&path) {
            for (i, key) in HUD_WIDGETS.iter().enumerate() {
                let name = format!("w.{key}");
                if let Some(value) = config.split(';').find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| v)) {
                    self.hud_features[i] = value.split(',').next().is_some_and(|enabled| enabled == "1");
                }
            }
        }
    }

    fn save_hud_feature(&mut self, index: usize, enabled: bool) {
        let Some(path) = self.hud_config_path.as_ref() else {
            self.status = Some(Status { message: "Basic HUD is not installed for this Horizon instance yet. Install it in Horizon, then refresh.".into(), error: true });
            return;
        };
        let mut pairs = fs::read_to_string(path).unwrap_or_default().split(';').map(str::to_owned).collect::<Vec<_>>();
        let key = format!("w.{}", HUD_WIDGETS[index]);
        let mut found = false;
        for pair in &mut pairs {
            if pair.split_once('=').is_some_and(|(name, _)| name == key) {
                if let Some((_, value)) = pair.split_once('=') {
                    let mut fields = value.split(',').map(str::to_owned).collect::<Vec<_>>();
                    if let Some(first) = fields.first_mut() { *first = if enabled { "1" } else { "0" }.into(); }
                    *pair = format!("{key}={}", fields.join(","));
                    found = true;
                }
                break;
            }
        }
        if !found {
            // The native mod understands the same compact widget record used by its editor.
            pairs.push(format!("{key}={},0.01,0.02,1,auto,FFFFFF,000000,47,1,2,0,1,0", if enabled { "1" } else { "0" }));
        }
        let result = fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
            .and_then(|()| fs::write(path, pairs.join(";")));
        match result {
            Ok(()) => { self.hud_features[index] = enabled; self.status = Some(Status { message: "Basic HUD setting saved. It will be active next time Horizon starts.".into(), error: false }); }
            Err(error) => self.status = Some(Status { message: format!("Could not save Basic HUD setting: {error}"), error: true }),
        }
    }

    /// Resolves the selected instance's `shaderpacks/` and `horizon-graphics.txt`, then
    /// lists every installed pack and the currently picked `pickedShaderPack` value.
    fn load_shader_state(&mut self) {
        self.shader_packs.clear();
        self.shader_picked.clear();
        self.shader_dir = None;
        self.shader_graphics = None;
        let (Some(version), Some(launcher)) = (self.selected_version(), self.official_launcher.as_deref()) else { return; };
        let Some(tag) = version.root.file_name().and_then(|name| name.to_str()) else { return; };
        let Ok(instance) = selected_instance(launcher, tag) else { return; };
        let instance_root = horizon_data_root().join("horizon/instances").join(instance);
        let graphics = instance_root.join("horizon-graphics.txt");
        let shader_dir = instance_root.join("shaderpacks");
        self.shader_picked = read_picked_shader(&graphics);
        self.shader_packs = scan_shader_packs(&shader_dir);
        self.shader_graphics = Some(graphics);
        self.shader_dir = Some(shader_dir);
    }

    /// Writes `pickedShaderPack:` in `horizon-graphics.txt`, leaving every other line untouched.
    fn set_picked_shader(&mut self, key: String) {
        if game_is_running() {
            self.status = Some(Status { message: "Close Horizon before changing shader packs; the game reads this setting at start.".into(), error: true });
            return;
        }
        let Some(path) = self.shader_graphics.clone() else {
            self.status = Some(Status { message: "No Horizon instance is selected for shader packs.".into(), error: true });
            return;
        };
        match write_picked_shader(&path, &key) {
            Ok(()) => {
                self.shader_picked = key.clone();
                let message = if key.is_empty() {
                    "Shaders disabled. Vanilla rendering returns next time Horizon starts.".to_owned()
                } else {
                    let title = self.shader_packs.iter().find(|pack| pack.key == key || pack.file == key)
                        .map(|pack| pack.title.clone())
                        .unwrap_or(key);
                    format!("Enabled {title}. It applies next time Horizon starts.")
                };
                self.status = Some(Status { message, error: false });
            }
            Err(error) => self.status = Some(Status { message: format!("Could not update pickedShaderPack: {error}"), error: true }),
        }
    }

    /// Copies a dropped `.zip` / `.hshader` / pack folder into `shaderpacks/` and enables it.
    fn install_shader_pack(&mut self, source: &Path) {
        if game_is_running() {
            self.status = Some(Status { message: "Close Horizon before installing shader packs.".into(), error: true });
            return;
        }
        let Some(dir) = self.shader_dir.clone() else {
            self.status = Some(Status { message: "No Horizon instance is selected for shader packs.".into(), error: true });
            return;
        };
        let Some(name) = source.file_name().and_then(|name| name.to_str()) else {
            self.status = Some(Status { message: "That path has no usable file name.".into(), error: true });
            return;
        };
        if !is_shader_pack_path(source) {
            self.status = Some(Status { message: format!("{name} is not a shader pack: expected a .zip, .hshader, or a folder with shaders/ or horizon-shaders.toml."), error: true });
            return;
        }
        let destination = dir.join(name);
        if destination.exists() {
            self.status = Some(Status { message: format!("{name} is already installed."), error: true });
            return;
        }
        let copied = if source.is_dir() {
            copy_dir_all(source, &destination)
        } else {
            fs::copy(source, &destination).map(|_| ()).map_err(|error| error.to_string())
        };
        if let Err(error) = copied {
            self.status = Some(Status { message: format!("Could not install {name}: {error}"), error: true });
            return;
        }
        self.load_shader_state();
        let key = self.shader_packs.iter().find(|pack| pack.file == name)
            .map(|pack| pack.key.clone())
            .unwrap_or_else(|| name.to_owned());
        self.set_picked_shader(key);
        if self.status.as_ref().is_some_and(|status| status.error) {
            return;
        }
        self.status = Some(Status { message: format!("Installed and enabled {name}. It applies next time Horizon starts."), error: false });
    }

    fn persist_selection(&mut self) {
        self.settings.selected_version = self
            .selected_version()
            .map(|version| path_to_string(&version.root));
        self.settings.selected_resource = self
            .selected_resource()
            .map(|resource| path_to_string(&resource.path));
        if let Some(account) = self.selected_account() {
            self.settings.selected_account = Some(account.uuid.clone());
        }

        if let Err(error) = save_settings(&self.settings) {
            self.status = Some(Status {
                message: format!("Could not save launcher settings: {error}"),
                error: true,
            });
        }
    }

    fn selected_version(&self) -> Option<&HorizonVersion> {
        self.selected_version
            .and_then(|index| self.versions.get(index))
    }

    fn selected_resource(&self) -> Option<&ResourceSet> {
        self.selected_version().and_then(|version| {
            self.selected_resource
                .and_then(|index| version.resources.get(index))
        })
    }

    fn selected_account(&self) -> Option<&AccountInfo> {
        self.selected_account
            .and_then(|index| self.accounts.get(index))
    }

    fn selected_account_id(&self) -> Option<String> {
        self.selected_account()
            .map(|account| account.uuid.clone())
            .or_else(|| self.settings.selected_account.clone())
    }

    fn load_accounts(&mut self) {
        let Some(launcher) = self.official_launcher.as_deref() else {
            self.status = Some(Status {
                message: "Could not find Horizon's sign-in helper. Set HORIZON_OFFICIAL_LAUNCHER to its AppImage or executable path.".into(),
                error: true,
            });
            return;
        };
        match accounts_from_official_launcher(launcher) {
            Ok(accounts) => {
                self.accounts = accounts;
                self.selected_account = self
                    .accounts
                    .iter()
                    .position(|account| {
                        self.settings.selected_account.as_deref() == Some(account.uuid.as_str())
                    })
                    .or_else(|| self.accounts.iter().position(|account| account.selected))
                    .or_else(|| (!self.accounts.is_empty()).then_some(0));
                self.persist_selection();
                self.status = Some(Status {
                    message: if self.accounts.is_empty() {
                        "No signed-in accounts were found.".into()
                    } else {
                        "Loaded account names from Horizon's sign-in helper.".into()
                    },
                    error: false,
                });
            }
            Err(error) => {
                self.status = Some(Status {
                    message: format!("Could not load Horizon accounts: {error}"),
                    error: true,
                });
            }
        }
    }

    fn launch(&mut self, ctx: &Context) {
        let Some(version) = self.selected_version().cloned() else {
            self.status = Some(Status {
                message: "No prepared Horizon client was found. Prepare a client build with your installed Horizon setup first.".into(),
                error: true,
            });
            return;
        };
        if self.selected_resource().is_none() {
            self.status = Some(Status {
                message: "This client has no prepared resource set.".into(),
                error: true,
            });
            return;
        }
        let Some(account) = self.selected_account_id() else {
            self.status = Some(Status {
                message: "Choose an account first. Use Load accounts to read names from Horizon's sign-in helper.".into(),
                error: true,
            });
            return;
        };
        let Some(launcher) = self.official_launcher.as_deref() else {
            self.status = Some(Status {
                message:
                    "Could not find Horizon's sign-in helper to create a one-time game session."
                        .into(),
                error: true,
            });
            return;
        };
        match spawn_game(
            &version,
            launcher,
            &account,
            &self.settings,
            self.capabilities,
        ) {
            Ok(child) => {
                self.status = Some(Status {
                    message: format!("Starting Horizon (PID {}).", child.id()),
                    error: false,
                });
                self.pending_game = Some(PendingGame {
                    child,
                    started: Instant::now(),
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            Err(error) => {
                self.status = Some(Status {
                    message: format!("Could not start Horizon: {error}"),
                    error: true,
                });
            }
        }
    }

    fn windowbar(&mut self, ctx: &Context) {
        let account = self.selected_account().map(|account| account.name.clone());
        egui::TopBottomPanel::top("windowbar")
            .exact_height(48.0)
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(10, 7, 24, 112))
                    .inner_margin(Margin::symmetric(16, 7))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22))),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    horizon_logo(ui, &self.logo, 28.0);
                    ui.add_space(6.0);
                    ui.label(RichText::new("NEBULA").size(14.0).strong().color(TEXT));
                    ui.label(RichText::new("CLIENT").size(10.5).strong().color(CORAL));
                    let drag_width = (ui.available_width() - 330.0).max(24.0);
                    let (drag_rect, drag) = ui.allocate_exact_size(Vec2::new(drag_width, 34.0), egui::Sense::click_and_drag());
                    if drag.drag_started() { ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
                    ui.painter().text(drag_rect.center(), egui::Align2::CENTER_CENTER, "Horizon  ·  Rust Edition", FontId::new(12.0, FontFamily::Proportional), Color32::from_rgba_unmultiplied(255, 255, 255, 140));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        window_control(ui, ctx, WindowAction::Close);
                        window_control(ui, ctx, WindowAction::Maximize);
                        window_control(ui, ctx, WindowAction::Minimize);
                        ui.add_space(8.0);
                        account_chip(ui, account.as_deref());
                    });
                });
            });
    }

    fn sidebar(&mut self, ctx: &Context) {
        // Collapse to an icon rail on narrow windows.
        let compact = ctx.screen_rect().width() < 1060.0;
        egui::SidePanel::left("sidebar")
            .exact_width(if compact { 78.0 } else { 196.0 })
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(10, 7, 24, 100))
                    .inner_margin(Margin::symmetric(12, 18))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22))),
            )
            .show(ctx, |ui| {
                if !compact {
                    ui.label(RichText::new("MENU").size(10.5).strong().color(MUTED));
                }
                ui.spacing_mut().item_spacing.y = 6.0;
                for (page, label) in Page::ALL {
                    if nav_item(ui, page, label, self.page == page, compact).clicked() {
                        self.page = page;
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(if compact { "26.3" } else { "Horizon 26.3  ·  Rust" })
                            .size(11.0)
                            .color(MUTED),
                    );
                });
            });
    }

    fn play_page(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let running = game_is_running();
        let starting = self.pending_game.is_some();

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("NEBULA  /  HORIZON 26.3").size(11.0).strong().color(CORAL));
                ui.add_space(3.0);
                ui.label(
                    RichText::new("Your world,\nat full speed.")
                        .font(FontId::new(36.0, FontFamily::Proportional))
                        .strong()
                        .color(TEXT),
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Native Minecraft client  ·  Rust edition").color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.vertical(|ui| {
                    let (state, color) = if running {
                        ("GAME RUNNING", SUCCESS)
                    } else if starting {
                        ("STARTING", CORAL)
                    } else {
                        ("READY TO PLAY", PURPLE_BRIGHT)
                    };
                    ui.label(RichText::new(state).size(11.0).strong().color(color));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            self.selected_version()
                                .map(|v| v.label.as_str())
                                .unwrap_or("No build selected"),
                        )
                        .color(TEXT),
                    );
                });
            });
        });
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        let performance_changed = glass_card(ui, inner_card(false), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Performance profile").strong().color(TEXT));
                    ui.label(
                        RichText::new("Dedicated shader cache and optional game-scoped tweaks")
                            .small()
                            .color(MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if self.settings.performance_mode { "ON" } else { "OFF" })
                            .small()
                            .strong()
                            .color(if self.settings.performance_mode { PURPLE_BRIGHT } else { MUTED }),
                    );
                    toggle_switch(ui, &mut self.settings.performance_mode)
                })
                .inner
                .changed()
            })
            .inner
        });
        if performance_changed {
            self.persist_selection();
        }
        ui.add_space(16.0);

        let previous_version = self.selected_version;
        labeled_combo(
            ui,
            "Build",
            "Choose a locally prepared Horizon build",
            "version",
            &mut self.selected_version,
            self.versions
                .iter()
                .enumerate()
                .map(|(index, version)| (index, version.label.as_str())),
        );
        if previous_version != self.selected_version {
            self.selected_resource = self
                .selected_version()
                .and_then(|version| (!version.resources.is_empty()).then_some(0));
            self.persist_selection();
        }

        let resource_choices: Vec<(usize, String)> = self
            .selected_version()
            .into_iter()
            .flat_map(|version| version.resources.iter().enumerate())
            .map(|(index, resource)| (index, resource.label.clone()))
            .collect();
        let previous_resource = self.selected_resource;
        labeled_combo(
            ui,
            "Resources",
            "Prepared game resources used by this build",
            "resources",
            &mut self.selected_resource,
            resource_choices
                .iter()
                .map(|(index, label)| (*index, label.as_str())),
        );

        let previous_account = self.selected_account;
        if self.accounts.is_empty() {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Account").strong().color(TEXT));
                    ui.label(
                        RichText::new("A one-time sign-in session is created only when you play.")
                            .small()
                            .color(MUTED),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            self.official_launcher.is_some(),
                            Button::new("Load accounts"),
                        )
                        .clicked()
                    {
                        self.load_accounts();
                    }
                });
            });
            ui.separator();
        } else {
            labeled_combo(
                ui,
                "Account",
                "A fresh one-time game session is requested just before launch.",
                "account",
                &mut self.selected_account,
                self.accounts
                    .iter()
                    .enumerate()
                    .map(|(index, account)| (index, account.name.as_str())),
            );
        }
        if previous_resource != self.selected_resource || previous_account != self.selected_account
        {
            self.persist_selection();
        }

        ui.add_space(14.0);
        // Same four requirements the launch path checks, surfaced before the click.
        let blocker = if self.official_launcher.is_none() {
            Some("Horizon's sign-in helper was not found.")
        } else if self.selected_version().is_none() {
            Some("Choose a Horizon build above.")
        } else if self.selected_resource().is_none() {
            Some("This build has no prepared resource set.")
        } else if self.selected_account_id().is_none() {
            Some("Load and choose an account to play.")
        } else {
            None
        };
        if play_button(ui, blocker.is_none() && !starting, starting) {
            self.launch(ctx);
        }
        if starting {
            ui.add_space(8.0);
            launch_progress(ui, ctx.input(|input| input.time) as f32);
            // Keeps the indeterminate sweep animating while Horizon starts.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else if let Some(reason) = blocker {
            ui.add_space(6.0);
            ui.label(RichText::new(reason).small().color(MUTED));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if quiet_action(ui, "↻", "Refresh builds").clicked() {
                self.refresh();
                self.status = Some(Status {
                    message: "Build list refreshed.".into(),
                    error: false,
                });
            }
            if running {
                ui.label(
                    RichText::new("A Horizon client is already running.").color(PURPLE_BRIGHT),
                );
            }
        });
        self.status_line(ui);
    }

    fn performance_page(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "PERFORMANCE",
            "Performance Mode",
            "Only supported, game-scoped settings are applied. No system-wide tuning is changed.",
        );
        glass_card(ui, inner_card(false), |ui| {
            ui.label(RichText::new("IN-GAME HUD  ·  BASIC HUD").small().strong().color(PURPLE_BRIGHT));
            ui.label(RichText::new(if self.hud_config_path.is_some() { "These switches update the installed native mod." } else { "Basic HUD settings are not available for the selected instance." }).small().color(MUTED));
            ui.add_space(8.0);
            egui::Grid::new("nebula_hud_features").num_columns(2).spacing([18.0, 5.0]).show(ui, |ui| {
                for (index, (name, hint)) in HUD_FEATURE_LABELS.iter().enumerate() {
                    let mut active = self.hud_features[index];
                    let response = ui.add_enabled(self.hud_config_path.is_some(), egui::Checkbox::new(&mut active, *name));
                    ui.label(RichText::new(*hint).small().color(MUTED));
                    ui.end_row();
                    if response.changed() { self.save_hud_feature(index, active); }
                }
            });
        });
        ui.add_space(24.0);

        let changed = setting_row(
            ui,
            &mut self.settings.performance_mode,
            "Performance Mode",
            "Keeps a dedicated Mesa shader cache for faster warm starts.",
            true,
        );
        let gamemode_changed = setting_row(
            ui,
            &mut self.settings.gamemode,
            "Feral GameMode",
            if self.capabilities.gamemode {
                "Run the game through gamemoderun."
            } else {
                "gamemoderun is not installed; this option is unavailable."
            },
            self.capabilities.gamemode,
        );
        let mangohud_changed = setting_row(
            ui,
            &mut self.settings.mangohud,
            "MangoHud",
            if self.capabilities.mangohud {
                "Run the game through MangoHud."
            } else {
                "mangohud is not installed; this option is unavailable."
            },
            self.capabilities.mangohud,
        );
        let io_changed = setting_row(
            ui,
            &mut self.settings.io_priority,
            "Loading I/O priority",
            if self.capabilities.ionice {
                "Uses ionice best-effort priority 0 for the client process."
            } else {
                "ionice is not available on this system."
            },
            self.capabilities.ionice,
        );

        ui.add_space(16.0);
        ui.label(
            RichText::new("Advanced renderer switches")
                .strong()
                .color(TEXT),
        );
        ui.label(
            RichText::new(
                "Leave these off unless you know the active renderer benefits from them.",
            )
            .small()
            .color(MUTED),
        );
        let gl_changed = setting_row(
            ui,
            &mut self.settings.opengl_threading,
            "Mesa OpenGL driver thread",
            "Sets MESA_GLTHREAD=true. It has no benefit for Vulkan and can regress some OpenGL paths.",
            true,
        );
        let nvidia_changed = setting_row(
            ui,
            &mut self.settings.nvidia_threading,
            "NVIDIA OpenGL driver thread",
            "Sets __GL_THREADED_OPTIMIZATIONS=1. It is NVIDIA/OpenGL-specific.",
            true,
        );

        if changed
            || gamemode_changed
            || mangohud_changed
            || io_changed
            || gl_changed
            || nvidia_changed
        {
            self.persist_selection();
        }
        self.status_line(ui);
    }

    fn shaders_page(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        // Install packs dropped anywhere on this page.
        let dropped: Vec<PathBuf> = ctx
            .input(|input| input.raw.dropped_files.clone())
            .into_iter()
            .filter_map(|file| file.path)
            .collect();
        for path in dropped {
            self.install_shader_pack(&path);
        }

        page_header(
            ui,
            "SHADERS",
            "Shader Packs",
            "Enable or disable a pack for the selected instance. Drop a .zip or .hshader anywhere on this page to install it.",
        );

        if self.shader_dir.is_none() {
            ui.label(
                RichText::new(
                    "No Horizon instance is selected, so shader packs cannot be loaded. Pick a build on Home, then refresh.",
                )
                .color(MUTED),
            );
            self.status_line(ui);
            return;
        }

        let running = game_is_running();
        let picked = self.shader_picked.clone();
        let packs = self.shader_packs.clone();

        // Current pick / off state.
        glass_card(ui, inner_card(false), |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ACTIVE").small().strong().color(PURPLE_BRIGHT));
                        if picked.is_empty() {
                            ui.label(RichText::new("Shaders off (vanilla rendering)").color(TEXT));
                        } else {
                            let title = packs
                                .iter()
                                .find(|pack| pack.key == picked || pack.file == picked)
                                .map(|pack| pack.title.clone())
                                .unwrap_or_else(|| picked.clone());
                            ui.label(RichText::new(title).strong().color(SUCCESS));
                            ui.label(RichText::new(&picked).small().monospace().color(MUTED));
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !picked.is_empty()
                            && ui
                                .add_enabled(
                                    !running,
                                    Button::new(RichText::new("Disable").size(14.0).color(TEXT))
                                        .fill(Color32::from_rgba_unmultiplied(137, 91, 231, 100))
                                        .stroke(Stroke::new(
                                            1.0_f32,
                                            Color32::from_rgba_unmultiplied(204, 174, 255, 105),
                                        ))
                                        .min_size(Vec2::new(110.0, 38.0)),
                                )
                                .clicked()
                        {
                            self.set_picked_shader(String::new());
                        }
                    });
                });
            });

        ui.add_space(10.0);
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        glass_card(ui, inner_card(hovering), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Drop a shader pack here to install")
                            .strong()
                            .color(TEXT),
                    );
                    ui.label(
                        RichText::new(".zip (shaders/ format) or .hshader (Horizon native)")
                            .small()
                            .color(MUTED),
                    );
                });
            });

        ui.add_space(16.0);
        ui.label(RichText::new("INSTALLED PACKS").small().strong().color(PURPLE_BRIGHT));
        ui.add_space(6.0);

        if packs.is_empty() {
            ui.label(
                RichText::new("No shader packs found in this instance's shaderpacks folder.")
                    .color(MUTED),
            );
        }

        let mut enable_request: Option<String> = None;
        for pack in &packs {
            let active = !picked.is_empty() && (picked == pack.key || picked == pack.file);
            glass_card(ui, inner_card(active), |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&pack.title).strong().color(TEXT));
                            ui.label(
                                RichText::new(format!(
                                    "{}  ·  {}",
                                    if pack.native { "Horizon native" } else { "shaders/ format" },
                                    pack.file
                                ))
                                .small()
                                .color(MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if active {
                                ui.label(RichText::new("ENABLED").small().strong().color(SUCCESS));
                            } else if ui
                                .add_enabled(
                                    !running,
                                    Button::new(RichText::new("Enable").size(14.0).color(TEXT))
                                        .fill(Color32::from_rgba_unmultiplied(137, 91, 231, 100))
                                        .stroke(Stroke::new(
                                            1.0_f32,
                                            Color32::from_rgba_unmultiplied(204, 174, 255, 105),
                                        ))
                                        .min_size(Vec2::new(110.0, 34.0)),
                                )
                                .clicked()
                            {
                                enable_request = Some(pack.key.clone());
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }

        if let Some(key) = enable_request {
            self.set_picked_shader(key);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if quiet_action(ui, "↻", "Refresh packs").clicked() {
                self.load_shader_state();
                self.status = Some(Status {
                    message: format!("Found {} shader pack(s).", self.shader_packs.len()),
                    error: false,
                });
            }
            if running {
                ui.label(
                    RichText::new("A Horizon client is running; close it to change shader packs.")
                        .color(PURPLE_BRIGHT),
                );
            }
        });
        self.status_line(ui);
    }

    fn diagnostics_page(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "DIAGNOSTICS", "Local installation", "");
        diagnostic_row(ui, "Horizon data", &horizon_data_root());
        diagnostic_row(ui, "Nebula cache", &cache_dir());
        diagnostic_row(ui, "Nebula settings", &settings_path());
        ui.add_space(18.0);
        ui.label(RichText::new(format!("Prepared builds: {}", self.versions.len())).color(TEXT));
        ui.label(RichText::new(format!("Loaded accounts: {}", self.accounts.len())).color(TEXT));
        ui.label(
            RichText::new(match &self.official_launcher {
                Some(path) => format!("Sign-in helper: {}", path.display()),
                None => "Sign-in helper: not found".into(),
            })
            .small()
            .color(MUTED),
        );
        ui.label(
            RichText::new(format!(
                "GameMode: {}  •  MangoHud: {}  •  ionice: {}",
                yes_no(self.capabilities.gamemode),
                yes_no(self.capabilities.mangohud),
                yes_no(self.capabilities.ionice)
            ))
            .color(MUTED),
        );
        ui.add_space(18.0);
        ui.label(
            RichText::new("Horizon's sign-in helper creates a one-time session at launch. Nebula never reads or stores your credentials.")
                .small()
                .color(MUTED),
        );
        self.status_line(ui);
    }

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "SETTINGS",
            "Nebula Client",
            "Minimal by design: no account service, updater, or telemetry loop.",
        );
        if setting_row(
            ui,
            &mut self.settings.close_after_launch,
            "Exit immediately after launch",
            "Closes Nebula after the native game process starts, leaving no launcher process in memory.",
            true,
        ) {
            self.persist_selection();
        }
        self.status_line(ui);
    }

    fn status_line(&self, ui: &mut egui::Ui) {
        if let Some(status) = &self.status {
            ui.add_space(14.0);
            let tint = if status.error { DANGER } else { SUCCESS };
            Frame::NONE
                .fill(tint.gamma_multiply(0.13))
                .stroke(Stroke::new(1.0_f32, tint.gamma_multiply(0.45)))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    let text = if status.error {
                        format!("Problem: {}", status.message)
                    } else {
                        status.message.clone()
                    };
                    ui.label(RichText::new(text).color(TEXT));
                });
        }
    }

    fn poll_game_start(&mut self, ctx: &Context) {
        let Some(pending) = self.pending_game.as_mut() else {
            return;
        };
        match pending.child.try_wait() {
            Ok(Some(status)) => {
                let code = status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".into());
                let log = cache_dir().join("last-game.log");
                self.status = Some(Status {
                    message: format!("Horizon exited during startup (code {code}). See {}", log.display()),
                    error: true,
                });
                self.pending_game = None;
            }
            Ok(None)
                if pending.started.elapsed() >= std::time::Duration::from_secs(8)
                    && game_is_running() =>
            {
                self.pending_game = None;
                if self.settings.close_after_launch {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    self.status = Some(Status {
                        message: "Horizon is running.".into(),
                        error: false,
                    });
                }
            }
            Ok(None) if pending.started.elapsed() >= std::time::Duration::from_secs(8) => {
                self.status = Some(Status {
                    message: "Horizon is preparing the game…".into(),
                    error: false,
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            Ok(None) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
            Err(error) => {
                self.status = Some(Status {
                    message: format!("Could not check Horizon startup: {error}"),
                    error: true,
                });
                self.pending_game = None;
            }
        }
    }
}

impl eframe::App for NebulaClient {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_game_start(ctx);
        self.sunset.paint(ctx);
        self.windowbar(ctx);
        self.sidebar(ctx);

        // No full-screen overlay: the sunset stays visible around a single glass panel.
        egui::CentralPanel::default()
            .frame(Frame::NONE.inner_margin(Margin::symmetric(32, 24)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let width = (ui.available_width() - 56.0).clamp(320.0, 900.0);
                    glass_card(ui, glass_frame(GLASS_ALPHA), |ui| {
                        ui.set_width(width);
                        match self.page {
                            Page::Play => self.play_page(ui, ctx),
                            Page::Performance => self.performance_page(ui),
                            Page::Shaders => self.shaders_page(ui, ctx),
                            Page::Diagnostics => self.diagnostics_page(ui),
                            Page::Settings => self.settings_page(ui),
                        }
                    });
                });
            });
    }
}

fn main() -> eframe::Result<()> {
    if env::args().skip(1).any(|argument| argument == "--diagnose") {
        print_diagnostics();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1160.0, 780.0])
            .with_min_inner_size([960.0, 640.0])
            .with_decorations(false)
            .with_drag_and_drop(true)
            .with_title("Nebula Client"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Nebula Client",
        options,
        Box::new(|cc| Ok(Box::new(NebulaClient::new(cc)))),
    )
}

fn configure_egui(ctx: &Context) {
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
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(205, 173, 255, 90));
    style.visuals.popup_shadow = egui::epaint::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(130) };
    style.spacing.combo_width = 330.0;
    style.visuals.selection.bg_fill = PURPLE;
    style.spacing.item_spacing = Vec2::new(11.0, 12.0);
    style.spacing.interact_size = Vec2::new(40.0, 36.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(10);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(10);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(10);
    style.visuals.window_corner_radius = egui::CornerRadius::same(16);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(10);
    ctx.set_style(style);
}


fn load_horizon_logo(ctx: &Context) -> egui::TextureHandle {
    let bytes = include_bytes!("../horizon-source/launcher/assets/logos/grass-block.png");
    let image = image::load_from_memory(bytes).expect("Horizon grass-block logo is a valid PNG").to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let rgba = image.as_raw();
    ctx.load_texture(
        "horizon-grass-block",
        egui::ColorImage::from_rgba_unmultiplied(size, rgba),
        egui::TextureOptions::NEAREST,
    )
}

fn horizon_logo(ui: &mut egui::Ui, texture: &egui::TextureHandle, size: f32) {
    ui.add(
        egui::Image::new((texture.id(), Vec2::splat(size)))
            .corner_radius(egui::CornerRadius::same(8)),
    );
}

fn quiet_action(ui: &mut egui::Ui, glyph: &str, label: &str) -> egui::Response {
    ui.add(
        Button::new(
            RichText::new(format!("{glyph}   {label}"))
                .size(14.0)
                .color(TEXT),
        )
        .fill(Color32::from_rgba_unmultiplied(137, 91, 231, 100))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(204, 174, 255, 105)))
        .min_size(Vec2::new(145.0, 38.0)),
    )
}

#[derive(Clone, Copy)]
enum WindowAction { Minimize, Maximize, Close }

fn window_control(ui: &mut egui::Ui, ctx: &Context, action: WindowAction) {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(40.0, 32.0), egui::Sense::click());
    let fill = match (action, response.hovered()) {
        (WindowAction::Close, true) => Color32::from_rgb(212, 70, 103),
        (_, true) => Color32::from_rgba_unmultiplied(255, 255, 255, 38),
        _ => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, 7.0, fill);
    let center = rect.center();
    let stroke = Stroke::new(1.4_f32, Color32::WHITE);
    match action {
        WindowAction::Minimize => { ui.painter().line_segment([center + Vec2::new(-5.0, 3.0), center + Vec2::new(5.0, 3.0)], stroke); }
        WindowAction::Maximize => {
            let r = egui::Rect::from_center_size(center, Vec2::splat(10.0));
            ui.painter().rect_stroke(r, 1.0, stroke, egui::StrokeKind::Inside);
        }
        WindowAction::Close => {
            ui.painter().line_segment([center + Vec2::splat(-4.0), center + Vec2::splat(4.0)], stroke);
            ui.painter().line_segment([center + Vec2::new(-4.0, 4.0), center + Vec2::new(4.0, -4.0)], stroke);
        }
    }
    if response.clicked() {
        match action {
            WindowAction::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            WindowAction::Maximize => {
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            WindowAction::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

fn toggle_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(46.0, 26.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        // Callers rely on `.changed()` to decide when to persist settings.
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *value, 0.14);
    let dim = if ui.is_enabled() { 1.0 } else { 0.4 };
    let track = lerp_color(Color32::from_rgb(54, 49, 68), VIOLET, t).gamma_multiply(dim);
    ui.painter().rect_filled(rect, 13.0, track);
    ui.painter().rect_stroke(rect, 13.0, Stroke::new(1.0_f32, Color32::from_white_alpha(40)), egui::StrokeKind::Inside);
    let x = rect.left() + 13.0 + (rect.width() - 26.0) * t;
    ui.painter().circle_filled(egui::pos2(x, rect.center().y + 1.5), 10.0, Color32::from_black_alpha(45));
    ui.painter().circle_filled(egui::pos2(x, rect.center().y), 9.5, Color32::WHITE.gamma_multiply(dim));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn labeled_combo<'a>(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    id: &str,
    selected: &mut Option<usize>,
    choices: impl IntoIterator<Item = (usize, &'a str)>,
) {
    let choices: Vec<(usize, &'a str)> = choices.into_iter().collect();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong().color(TEXT));
            ui.label(RichText::new(description).small().color(MUTED));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let selected_label = selected
                .and_then(|index| {
                    choices
                        .iter()
                        .find(|(choice, _)| *choice == index)
                        .map(|(_, label)| *label)
                })
                .unwrap_or("Not found");
            ComboBox::from_id_salt(id)
                .selected_text(RichText::new(selected_label).size(14.0).color(TEXT))
                .width(330.0)
                .height(288.0)
                .icon(|ui, rect, visuals, open, _| {
                    let center = rect.right_center() - Vec2::new(14.0, 0.0);
                    let offset = if open { -1.0 } else { 1.0 };
                    let color = visuals.fg_stroke.color.gamma_multiply(0.72);
                    ui.painter().line_segment(
                        [center + Vec2::new(-4.0, -2.0 * offset), center + Vec2::new(0.0, 2.0 * offset)],
                        Stroke::new(1.5_f32, color),
                    );
                    ui.painter().line_segment(
                        [center + Vec2::new(0.0, 2.0 * offset), center + Vec2::new(4.0, -2.0 * offset)],
                        Stroke::new(1.5_f32, color),
                    );
                })
                .show_ui(ui, |ui| {
                    ui.spacing_mut().interact_size.y = 36.0;
                    ui.spacing_mut().button_padding = Vec2::new(12.0, 5.0);
                    for (index, label) in &choices {
                        ui.selectable_value(
                            selected,
                            Some(*index),
                            RichText::new(*label).size(14.0).color(TEXT),
                        );
                    }
                });
        });
    });
    ui.separator();
}

fn setting_row(
    ui: &mut egui::Ui,
    value: &mut bool,
    title: &str,
    description: &str,
    enabled: bool,
) -> bool {
    let changed = ui
        .horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(title)
                        .strong()
                        .color(if enabled { TEXT } else { MUTED }),
                );
                ui.label(RichText::new(description).small().color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(enabled, |ui| toggle_switch(ui, value)).inner
            })
            .inner
            .changed()
        })
        .inner;
    ui.separator();
    changed
}

fn diagnostic_row(ui: &mut egui::Ui, label: &str, value: &Path) {
    ui.label(RichText::new(label).strong().color(TEXT));
    ui.label(
        RichText::new(value.display().to_string())
            .monospace()
            .small()
            .color(MUTED),
    );
    ui.add_space(8.0);
}

// ---------------------------------------------------------------------------------------------
// Visual components. Everything below is presentation only; launch logic lives elsewhere.
// ---------------------------------------------------------------------------------------------

/// Linear blend between two opaque colours.
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// The main translucent panel that holds a page. The sunset remains visible through it.
fn glass_frame(alpha: u8) -> Frame {
    Frame::NONE
        .fill(Color32::from_rgba_unmultiplied(16, 11, 36, alpha))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(34)))
        .corner_radius(egui::CornerRadius::same(18))
        .inner_margin(Margin::symmetric(28, 24))
        .shadow(egui::epaint::Shadow {
            offset: [0, 10],
            blur: 30,
            spread: 0,
            color: Color32::from_black_alpha(70),
        })
}

/// A lighter card nested inside the main panel. `highlight` marks hover or active state.
fn inner_card(highlight: bool) -> Frame {
    Frame::NONE
        .fill(if highlight {
            Color32::from_rgba_unmultiplied(139, 91, 249, 52)
        } else {
            Color32::from_white_alpha(12)
        })
        .stroke(Stroke::new(
            1.0_f32,
            if highlight {
                Color32::from_rgba_unmultiplied(214, 190, 255, 140)
            } else {
                Color32::from_white_alpha(28)
            },
        ))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 13))
}

/// Shows `frame` around the contents and adds a faint top-edge highlight so it reads as glass.
fn glass_card<R>(ui: &mut egui::Ui, frame: Frame, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let output = frame.show(ui, add_contents);
    let rect = output.response.rect;
    ui.painter().line_segment(
        [rect.left_top() + Vec2::new(20.0, 0.5), rect.right_top() + Vec2::new(-20.0, 0.5)],
        Stroke::new(1.0_f32, Color32::from_white_alpha(52)),
    );
    output.inner
}

fn page_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(RichText::new(eyebrow).size(11.0).strong().color(CORAL));
    ui.add_space(4.0);
    ui.label(
        RichText::new(title)
            .font(FontId::new(32.0, FontFamily::Proportional))
            .strong()
            .color(TEXT),
    );
    if !subtitle.is_empty() {
        ui.label(RichText::new(subtitle).color(MUTED));
    }
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(8.0);
}

fn account_chip(ui: &mut egui::Ui, account: Option<&str>) {
    Frame::NONE
        .fill(Color32::from_white_alpha(16))
        .stroke(Stroke::new(1.0_f32, Color32::from_white_alpha(34)))
        .corner_radius(egui::CornerRadius::same(16))
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (avatar, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                let initial = account
                    .and_then(|name| name.chars().next())
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "?".to_string());
                ui.painter().circle_filled(
                    avatar.center(),
                    11.0,
                    if account.is_some() { VIOLET } else { Color32::from_rgb(70, 62, 96) },
                );
                ui.painter().text(
                    avatar.center(),
                    egui::Align2::CENTER_CENTER,
                    initial,
                    FontId::new(12.0, FontFamily::Proportional),
                    Color32::WHITE,
                );
                ui.label(RichText::new(account.unwrap_or("Account not loaded")).size(13.0).color(TEXT));
            });
        });
}

/// One sidebar destination: painted icon, label, animated hover and selection states.
fn nav_item(ui: &mut egui::Ui, page: Page, label: &str, selected: bool, compact: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), egui::Sense::click());
    let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let active = ui.ctx().animate_bool_with_time(response.id.with("active"), selected, 0.18);
    let painter = ui.painter();
    painter.rect_filled(rect, 12.0, Color32::from_white_alpha((hover * 16.0) as u8));
    if active > 0.0 {
        painter.rect_filled(rect, 12.0, Color32::from_rgba_unmultiplied(139, 91, 249, (active * 110.0) as u8));
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(214, 190, 255, (active * 100.0) as u8)),
            egui::StrokeKind::Inside,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 12.0), Vec2::new(3.0, 20.0)),
            2.0,
            CORAL.gamma_multiply(active),
        );
    }
    let color = lerp_color(MUTED, TEXT, active.max(hover * 0.7));
    let icon_x = if compact { rect.center().x } else { rect.left() + 26.0 };
    draw_nav_icon(painter, page, egui::pos2(icon_x, rect.center().y), color);
    if !compact {
        painter.text(
            egui::pos2(rect.left() + 50.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::new(14.5, FontFamily::Proportional),
            color,
        );
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if compact {
        response.on_hover_text(label)
    } else {
        response
    }
}

/// Small line icons drawn with the painter so they stay crisp and need no icon font.
fn draw_nav_icon(painter: &egui::Painter, page: Page, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.6_f32, color);
    let p = |x: f32, y: f32| c + Vec2::new(x, y);
    match page {
        Page::Play => {
            painter.line_segment([p(-8.0, 0.0), p(0.0, -8.0)], s);
            painter.line_segment([p(0.0, -8.0), p(8.0, 0.0)], s);
            painter.rect_stroke(
                egui::Rect::from_min_max(p(-5.5, 0.0), p(5.5, 8.0)),
                1.5,
                s,
                egui::StrokeKind::Inside,
            );
        }
        Page::Performance => {
            painter.line_segment([p(2.5, -9.0), p(-4.0, 1.0)], s);
            painter.line_segment([p(-4.0, 1.0), p(4.0, 1.0)], s);
            painter.line_segment([p(4.0, 1.0), p(-2.5, 9.0)], s);
        }
        Page::Shaders => {
            painter.circle_stroke(c, 4.0, s);
            for i in 0..8 {
                let (sin, cos) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
                painter.line_segment([p(cos * 6.5, sin * 6.5), p(cos * 9.0, sin * 9.0)], s);
            }
        }
        Page::Diagnostics => {
            for (y, len) in [(-6.0_f32, 18.0_f32), (0.0, 12.0), (6.0, 15.0)] {
                painter.line_segment([p(-9.0, y), p(-9.0 + len, y)], s);
            }
        }
        Page::Settings => {
            for (y, knob) in [(-6.0_f32, -3.0_f32), (0.0, 4.0), (6.0, -1.0)] {
                painter.line_segment([p(-9.0, y), p(9.0, y)], s);
                painter.circle_filled(p(knob, y), 2.6, color);
            }
        }
    }
}

/// The primary call to action: a violet-to-coral sunset gradient with a soft glow.
/// Returns true when it was clicked while enabled.
fn play_button(ui: &mut egui::Ui, enabled: bool, starting: bool) -> bool {
    let size = Vec2::new(ui.available_width().min(360.0), 62.0);
    let (full, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let hover = ui.ctx().animate_bool_with_time(response.id.with("hover"), response.hovered() && enabled, 0.15);
    let pressed = enabled && response.is_pointer_button_down_on();
    let rect = if pressed { full.shrink(1.5) } else { full };
    let r: u8 = 16;
    let (c1, c2) = if enabled {
        (
            lerp_color(VIOLET, Color32::from_rgb(172, 128, 255), hover),
            lerp_color(CORAL, Color32::from_rgb(255, 160, 118), hover),
        )
    } else {
        (Color32::from_rgb(58, 50, 84), Color32::from_rgb(78, 62, 90))
    };
    let painter = ui.painter();
    if enabled {
        painter.rect_filled(
            full.expand(2.0 + 4.0 * hover),
            egui::CornerRadius::same(r + 4),
            CORAL.gamma_multiply(0.10 + 0.14 * hover),
        );
    }
    // Flat end caps keep the rounded corners; the gradient mesh fills the span between them.
    let mid = rect.center().x;
    painter.rect_filled(
        egui::Rect::from_min_max(rect.left_top(), egui::pos2(mid, rect.bottom())),
        egui::CornerRadius { nw: r, ne: 0, sw: r, se: 0 },
        c1,
    );
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(mid, rect.top()), rect.right_bottom()),
        egui::CornerRadius { nw: 0, ne: r, sw: 0, se: r },
        c2,
    );
    let g = egui::Rect::from_min_max(
        egui::pos2(rect.left() + r as f32, rect.top()),
        egui::pos2(rect.right() - r as f32, rect.bottom()),
    );
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(g.left_top(), c1);
    mesh.colored_vertex(g.right_top(), c2);
    mesh.colored_vertex(g.right_bottom(), c2);
    mesh.colored_vertex(g.left_bottom(), c1);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_filled(
        egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.right(), rect.center().y)),
        egui::CornerRadius { nw: r, ne: r, sw: 0, se: 0 },
        Color32::from_white_alpha(if enabled { 28 } else { 10 }),
    );
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(r),
        Stroke::new(1.0_f32, Color32::from_white_alpha(if enabled { 80 } else { 24 })),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if starting { "Starting Horizon…" } else { "▶   Play" },
        FontId::new(19.0, FontFamily::Proportional),
        if enabled { Color32::WHITE } else { MUTED },
    );
    let clicked = enabled && response.clicked();
    if enabled {
        let _ = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    clicked
}

/// Indeterminate indicator shown while Horizon prepares the game. Horizon reports no real
/// percentage, so this sweeps instead of pretending to measure progress.
fn launch_progress(ui: &mut egui::Ui, time: f32) {
    let width = ui.available_width().min(360.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_white_alpha(28));
    let segment = rect.width() * 0.32;
    let phase = (time * 0.7).fract();
    let start = rect.left() - segment + (rect.width() + segment) * phase;
    let left = start.max(rect.left());
    let right = (start + segment).min(rect.right());
    if right > left {
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(right, rect.bottom())),
            3.0,
            lerp_color(VIOLET, CORAL, phase),
        );
    }
}

fn discover_versions() -> Vec<HorizonVersion> {
    let versions_root = horizon_data_root().join("horizon-launcher/versions");
    let mut versions = Vec::new();
    let Ok(entries) = fs::read_dir(versions_root) else {
        return versions;
    };

    for entry in entries.flatten() {
        let root = entry.path();
        if !root.is_dir() {
            continue;
        }
        let client = root.join("horizon-client");
        if !client.is_file() {
            continue;
        }
        let resources = discover_resources(&root);
        if resources.is_empty() {
            continue;
        }
        let modified = modified_at(&client);
        let folder_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown build")
            .to_owned();
        let label = if folder_name.starts_with("native-") {
            "Nebula native build".to_owned()
        } else {
            folder_name
        };
        versions.push(HorizonVersion {
            label,
            root,
            client,
            resources,
            modified,
        });
    }
    versions.sort_by_key(|version| std::cmp::Reverse(version.modified));
    versions
}

fn discover_resources(version_root: &Path) -> Vec<ResourceSet> {
    let root = version_root.join(".local/horizon-client");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut resources = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("horizon-resources.json").is_file() {
            continue;
        }
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("resources")
            .to_owned();
        resources.push(ResourceSet { label, path });
    }
    resources.sort_by(|left, right| right.label.cmp(&left.label));
    resources
}

fn spawn_game(
    version: &HorizonVersion,
    launcher: &Path,
    account: &str,
    settings: &LauncherSettings,
    capabilities: Capabilities,
) -> Result<Child, String> {
    if !version.client.is_file() {
        return Err("the selected horizon-client binary no longer exists".into());
    }
    let tag = version
        .root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "the selected Horizon build has an invalid version name".to_owned())?;
    let instance = selected_instance(launcher, tag)?;

    // Managed Horizon builds must be started through Horizon's Play path so it can
    // acquire the installed cache leases, resolve instance settings and renew sessions.
    let mut argv = vec![
        launcher.as_os_str().to_owned(),
        OsString::from("--json"),
        OsString::from("play"),
        OsString::from("--instance"),
        OsString::from(instance),
        OsString::from("--version"),
        OsString::from(tag),
        OsString::from("--account"),
        OsString::from(account),
    ];
    if settings.gamemode && capabilities.gamemode {
        argv.insert(0, OsString::from("gamemoderun"));
    }
    if settings.io_priority && capabilities.ionice {
        argv.splice(
            0..0,
            [
                OsString::from("ionice"),
                OsString::from("-c"),
                OsString::from("2"),
                OsString::from("-n"),
                OsString::from("0"),
                OsString::from("--"),
            ],
        );
    }

    let Some(program) = argv.first().cloned() else {
        return Err("the launch command was empty".into());
    };
    let mut command = Command::new(program);
    command.args(argv.iter().skip(1));
    command.env("NEBULA_CLIENT", "1");

    fs::create_dir_all(cache_dir())
        .map_err(|error| format!("could not create Nebula log directory: {error}"))?;
    let log = fs::File::create(cache_dir().join("last-game.log"))
        .map_err(|error| format!("could not create game log: {error}"))?;
    command.stdout(Stdio::from(log.try_clone().map_err(|error| error.to_string())?));
    command.stderr(Stdio::from(log));

    if settings.performance_mode {
        let shader_cache = cache_dir().join("mesa-shader-cache");
        fs::create_dir_all(&shader_cache)
            .map_err(|error| format!("could not create shader-cache directory: {error}"))?;
        command.env("MESA_SHADER_CACHE_DIR", shader_cache);
    }
    if settings.opengl_threading {
        command.env("MESA_GLTHREAD", "true");
    }
    if settings.nvidia_threading {
        command.env("__GL_THREADED_OPTIMIZATIONS", "1");
    }
    if settings.mangohud && capabilities.mangohud {
        command.env("MANGOHUD", "1");
    }

    command.spawn().map_err(|error| error.to_string())
}

fn selected_instance(launcher: &Path, tag: &str) -> Result<String, String> {
    let output = Command::new(launcher)
        .args(["--json", "instance", "list"])
        .output()
        .map_err(|error| format!("could not inspect Horizon instances: {error}"))?;
    if !output.status.success() {
        return Err("Horizon could not list its installed instances".into());
    }
    let instances: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Horizon returned invalid instance data: {error}"))?;
    let items = instances
        .as_array()
        .ok_or_else(|| "Horizon returned an invalid instance list".to_owned())?;
    let selected = items
        .iter()
        .find(|item| item.get("selected").and_then(serde_json::Value::as_bool) == Some(true)
            && item.get("version").and_then(serde_json::Value::as_str) == Some(tag))
        .or_else(|| items.iter().find(|item| item.get("version").and_then(serde_json::Value::as_str) == Some(tag)))
        .ok_or_else(|| format!("No Horizon instance uses build {tag}. Select the Nebula Client instance in Horizon, then refresh Nebula."))?;
    selected
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Horizon's selected instance has no id".into())
}

#[cfg(test)]
fn launch_argv(
    client: &Path,
    resources: &Path,
    session: &Path,
    settings: &LauncherSettings,
    capabilities: Capabilities,
) -> Vec<OsString> {
    let mut argv = vec![
        client.into(),
        OsString::from("--resources"),
        resources.into(),
        OsString::from("--session-file"),
        session.into(),
    ];

    if settings.io_priority && capabilities.ionice {
        let mut wrapped = vec![
            OsString::from("ionice"),
            OsString::from("-c"),
            OsString::from("2"),
            OsString::from("-n"),
            OsString::from("0"),
            OsString::from("--"),
        ];
        wrapped.append(&mut argv);
        argv = wrapped;
    }
    if settings.gamemode && capabilities.gamemode {
        let mut wrapped = vec![OsString::from("gamemoderun")];
        wrapped.append(&mut argv);
        argv = wrapped;
    }
    if settings.mangohud && capabilities.mangohud {
        let mut wrapped = vec![OsString::from("mangohud")];
        wrapped.append(&mut argv);
        argv = wrapped;
    }
    argv
}

fn game_is_running() -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            return false;
        };
        if name == std::process::id() {
            return false;
        }
        fs::read_link(entry.path().join("exe"))
            .ok()
            .and_then(|path| path.file_name().map(|file| file == "horizon-client"))
            .unwrap_or(false)
    })
}

fn command_exists(command: &str) -> bool {
    command_in_path(command).is_some()
}

fn command_in_path(command: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|folder| folder.join(command))
            .find(|path| path.is_file())
    })
}

fn discover_official_launcher() -> Option<PathBuf> {
    if let Some(path) = env::var_os("HORIZON_OFFICIAL_LAUNCHER").map(PathBuf::from) {
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(home) = home_dir() {
        for candidate in [
            home.join("horizon-launcher-linux-x86_64.AppImage"),
            home.join("Applications/horizon-launcher-linux-x86_64.AppImage"),
            home.join(".local/bin/horizon-launcher"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    command_in_path("horizon-launcher")
}

fn accounts_from_official_launcher(launcher: &Path) -> Result<Vec<AccountInfo>, String> {
    let value = official_launcher_json(launcher, &["account", "list"])?;
    serde_json::from_value::<AccountList>(value)
        .map(|accounts| accounts.accounts)
        .map_err(|error| format!("Horizon sign-in helper returned invalid account data: {error}"))
}

fn official_launcher_json(
    launcher: &Path,
    arguments: &[&str],
) -> Result<serde_json::Value, String> {
    let output = Command::new(launcher)
        .arg("--json")
        .args(arguments)
        .output()
        .map_err(|error| format!("could not run {}: {error}", launcher.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .next()
            .unwrap_or("Horizon sign-in helper command failed");
        return Err(detail.chars().take(240).collect());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Horizon sign-in helper returned invalid JSON: {error}"))
}

fn modified_at(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn find_path_index<T>(
    values: &[T],
    selected: Option<&str>,
    path: impl Fn(&T) -> &Path,
) -> Option<usize> {
    let selected = selected?;
    values
        .iter()
        .position(|value| path(value).as_os_str() == selected)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn horizon_data_root() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn config_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nebula-client")
}

fn cache_dir() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nebula-client")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// The current `pickedShaderPack:` value; empty when unset or shaders are off.
fn read_picked_shader(path: &Path) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("pickedShaderPack:")
                    .map(str::trim)
                    .map(str::to_owned)
            })
        })
        .unwrap_or_default()
}

/// Replaces or appends only the `pickedShaderPack:` line; every other line is preserved.
fn write_picked_shader(path: &Path, value: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut replaced = false;
    for line in &mut lines {
        if line.starts_with("pickedShaderPack:") {
            *line = format!("pickedShaderPack:{value}");
            replaced = true;
        }
    }
    if !replaced {
        lines.push(format!("pickedShaderPack:{value}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    let temporary = path.with_extension("txt.tmp");
    fs::write(&temporary, out).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

/// Scans `shaderpacks/` for Horizon-native and classic `shaders/` packs.
fn scan_shader_packs(dir: &Path) -> Vec<ShaderPackEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    let mut packs = Vec::new();
    for path in paths {
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            let manifest = path.join("horizon-shaders.toml");
            if manifest.is_file() {
                let text = fs::read_to_string(&manifest).unwrap_or_default();
                let id = toml_root_string(&text, "id").unwrap_or_else(|| file.to_owned());
                let title = toml_root_string(&text, "name").unwrap_or_else(|| file.to_owned());
                packs.push(ShaderPackEntry {
                    file: file.to_owned(),
                    key: format!("horizon:{id}"),
                    title,
                    native: true,
                });
            } else if path.join("shaders").is_dir() {
                packs.push(ShaderPackEntry {
                    file: file.to_owned(),
                    key: file.to_owned(),
                    title: file.to_owned(),
                    native: false,
                });
            }
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("hshader")) {
            // The .hshader itself is a zipped Horizon pack; the filename is its identity.
            packs.push(ShaderPackEntry {
                file: file.to_owned(),
                key: file.to_owned(),
                title: file.trim_end_matches(".hshader").to_owned(),
                native: true,
            });
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("zip")) {
            packs.push(ShaderPackEntry {
                file: file.to_owned(),
                key: file.to_owned(),
                title: file.to_owned(),
                native: false,
            });
        }
    }
    // Horizon's own packs first, then classic; each group by title (matches the game's list).
    packs.sort_by(|a, b| {
        (!a.native, a.title.to_lowercase()).cmp(&(!b.native, b.title.to_lowercase()))
    });
    packs
}

/// A top-level (unindented, before any `[section]`) `key = "value"` line, if present.
fn toml_root_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        if line.starts_with([' ', '\t']) {
            continue;
        }
        if line.trim_start().starts_with('[') {
            break;
        }
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        if !rest.starts_with([' ', '\t', '=']) {
            continue;
        }
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = rest.trim();
        let unquoted = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')))?;
        if !unquoted.is_empty() {
            return Some(unquoted.to_owned());
        }
    }
    None
}

fn is_shader_pack_path(path: &Path) -> bool {
    if path.is_dir() {
        return path.join("shaders").is_dir() || path.join("horizon-shaders.toml").is_file();
    }
    path.is_file()
        && path.extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("hshader")
        })
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn load_settings() -> Result<LauncherSettings, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(LauncherSettings::default());
    }
    let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn save_settings(settings: &LauncherSettings) -> Result<(), String> {
    let folder = config_dir();
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    let temporary = folder.join("settings.json.tmp");
    fs::write(&temporary, text).map_err(|error| error.to_string())?;
    fs::rename(&temporary, settings_path()).map_err(|error| error.to_string())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "available"
    } else {
        "not installed"
    }
}

fn print_diagnostics() {
    let versions = discover_versions();
    let capabilities = Capabilities::detect();
    println!("Nebula Client diagnostic");
    println!("Horizon data: {}", horizon_data_root().display());
    println!("Prepared builds: {}", versions.len());
    for version in versions {
        println!(
            "  {} — {} resource set(s) — {}",
            version.label,
            version.resources.len(),
            version.client.display()
        );
    }
    println!(
        "Sign-in helper: {}",
        discover_official_launcher()
            .map_or_else(|| "not found".into(), |path| path.display().to_string())
    );
    println!(
        "Tools: GameMode={}, MangoHud={}, ionice={}",
        yes_no(capabilities.gamemode),
        yes_no(capabilities.mangohud),
        yes_no(capabilities.ionice)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(arguments: Vec<OsString>) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn direct_command_keeps_the_native_client_contract() {
        let settings = LauncherSettings {
            performance_mode: false,
            gamemode: false,
            mangohud: false,
            io_priority: false,
            ..LauncherSettings::default()
        };
        let arguments = launch_argv(
            Path::new("/build/horizon-client"),
            Path::new("/build/resources"),
            Path::new("/sessions/default.json"),
            &settings,
            Capabilities {
                gamemode: true,
                mangohud: true,
                ionice: true,
            },
        );
        assert_eq!(
            strings(arguments),
            vec![
                "/build/horizon-client",
                "--resources",
                "/build/resources",
                "--session-file",
                "/sessions/default.json",
            ]
        );
    }

    #[test]
    fn enabled_tools_wrap_the_client_without_changing_game_arguments() {
        let settings = LauncherSettings {
            gamemode: true,
            mangohud: true,
            io_priority: true,
            ..LauncherSettings::default()
        };
        let arguments = launch_argv(
            Path::new("/build/horizon-client"),
            Path::new("/build/resources"),
            Path::new("/sessions/default.json"),
            &settings,
            Capabilities {
                gamemode: true,
                mangohud: true,
                ionice: true,
            },
        );
        // Outermost wrapper is applied last: mangohud, then gamemoderun, then ionice.
        assert_eq!(
            strings(arguments),
            vec![
                "mangohud",
                "gamemoderun",
                "ionice",
                "-c",
                "2",
                "-n",
                "0",
                "--",
                "/build/horizon-client",
                "--resources",
                "/build/resources",
                "--session-file",
                "/sessions/default.json",
            ]
        );
    }

    #[test]
    fn picked_shader_write_preserves_every_other_graphics_line() {
        let dir = std::env::temp_dir().join(format!("nebula-shader-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("horizon-graphics.txt");
        fs::write(
            &path,
            "preset:performance\nlodDistance:128\nresolutionScale:80\npickedShaderPack:old.zip\ndynamicResolution:true\ntargetFps:120\n",
        )
        .unwrap();

        write_picked_shader(&path, "ComplementaryReimagined_r5.10-horizon.zip").unwrap();
        let enabled = fs::read_to_string(&path).unwrap();
        assert!(enabled.contains("preset:performance"));
        assert!(enabled.contains("lodDistance:128"));
        assert!(enabled.contains("resolutionScale:80"));
        assert!(enabled.contains("dynamicResolution:true"));
        assert!(enabled.contains("targetFps:120"));
        assert!(enabled.contains("pickedShaderPack:ComplementaryReimagined_r5.10-horizon.zip"));
        assert!(!enabled.contains("old.zip"));
        assert_eq!(
            read_picked_shader(&path),
            "ComplementaryReimagined_r5.10-horizon.zip"
        );

        write_picked_shader(&path, "").unwrap();
        let disabled = fs::read_to_string(&path).unwrap();
        assert!(disabled.contains("preset:performance"));
        assert!(disabled.contains("lodDistance:128"));
        assert!(disabled.contains("pickedShaderPack:\n"));
        assert_eq!(read_picked_shader(&path), "");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn classic_and_native_packs_scan_with_the_right_keys() {
        let dir = std::env::temp_dir().join(format!("nebula-pack-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("classic/shaders")).unwrap();
        fs::create_dir_all(dir.join("native")).unwrap();
        fs::write(
            dir.join("native/horizon-shaders.toml"),
            "schema = 1\nid = \"soft_light\"\nname = \"Soft Light\"\napi = \"0.1\"\n",
        )
        .unwrap();
        fs::write(dir.join("ComplementaryReimagined_r5.10-horizon.zip"), b"PK").unwrap();
        fs::write(dir.join("bonus.hshader"), b"PK").unwrap();
        fs::write(dir.join("not-a-pack.txt"), b"no").unwrap();

        let packs = scan_shader_packs(&dir);
        let keys: Vec<&str> = packs.iter().map(|pack| pack.key.as_str()).collect();
        assert!(keys.contains(&"horizon:soft_light"), "{keys:?}");
        assert!(keys.contains(&"ComplementaryReimagined_r5.10-horizon.zip"), "{keys:?}");
        assert!(keys.contains(&"classic"), "{keys:?}");
        assert!(keys.contains(&"bonus.hshader"), "{keys:?}");
        assert!(!keys.contains(&"not-a-pack.txt"), "{keys:?}");
        let native = packs.iter().find(|pack| pack.file == "native").unwrap();
        assert!(native.native);
        assert_eq!(native.title, "Soft Light");

        fs::remove_dir_all(&dir).ok();
    }
}
