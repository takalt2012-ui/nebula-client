//! A deliberately small independent interface for an installed Horizon build.
//! It delegates managed game starts to Horizon's own `play` command so cache leases,
//! instance configuration and account-session handoff remain correct.


use crate::scene;
mod graphics;
mod pages;
mod platform;
use crate::ui::components::{load_horizon_logo, main_panel_frame, panel_card, Page};
use crate::ui::theme::configure_egui;
use graphics::{write_setting, GfxSettings};
use platform::{
    accounts_from_official_launcher, cache_dir, command_exists, copy_dir_all,
    discover_official_launcher, discover_versions, find_path_index, game_is_running,
    horizon_data_root, is_shader_pack_path, load_settings, path_to_string,
    print_diagnostics, read_picked_shader, save_settings, scan_shader_packs, selected_instance,
    spawn_game, write_picked_shader,
};
#[cfg(test)]
use crate::ui::theme::THEME;
#[cfg(test)]
use graphics::{read_setting, set_setting_text, CullingMode, FramePacing, GpuCulling, LeafCulling};
#[cfg(test)]
use platform::launch_argv;
use eframe::egui::{self, Context, Frame, Margin};
use serde::{Deserialize, Serialize};
use std::env;
#[cfg(test)]
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Instant, SystemTime};

const PANEL_OPACITY: u8 = 204;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusKind {
    Success,
    Error,
}

struct Status {
    message: String,
    kind: StatusKind,
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
    /// Leaf, transparency and culling settings read from the selected instance.
    gfx: Option<GfxSettings>,
    /// `options.txt` and `horizon-graphics.txt` of the selected instance.
    gfx_paths: Option<(PathBuf, PathBuf)>,
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
            gfx: None,
            gfx_paths: None,
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
        self.hud_config_path = Some(path);
        self.hud_features = [true, true, true, true, true, true, true, true, true, false, false, false, true, true];
        let Some(config_path) = self.hud_config_path.as_deref() else {
            return;
        };
        let Ok(config) = fs::read_to_string(config_path) else {
            return;
        };
        for (index, key) in HUD_WIDGETS.iter().enumerate() {
            let setting_key = format!("w.{key}");
            if let Some(value) = config.split(';').find_map(|pair| {
                pair.split_once('=')
                    .filter(|(stored_key, _)| *stored_key == setting_key.as_str())
                    .map(|(_, value)| value)
            }) {
                self.hud_features[index] = value
                    .split(',')
                    .next()
                    .is_some_and(|enabled| enabled == "1");
            }
        }
    }

    fn save_hud_feature(&mut self, index: usize, enabled: bool) {
        let Some(path) = self.hud_config_path.as_ref() else {
            self.status = Some(Status { message: "Basic HUD is not installed for this Horizon instance yet. Install it in Horizon, then refresh.".into(), kind: StatusKind::Error });
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
            Ok(()) => { self.hud_features[index] = enabled; self.status = Some(Status { message: "Basic HUD setting saved. It will be active next time Horizon starts.".into(), kind: StatusKind::Success }); }
            Err(error) => self.status = Some(Status { message: format!("Could not save Basic HUD setting: {error}"), kind: StatusKind::Error }),
        }
    }

    /// Resolves the selected instance's `shaderpacks/` and `horizon-graphics.txt`, then
    /// lists every installed pack and the currently picked `pickedShaderPack` value.
    fn load_shader_state(&mut self) {
        self.shader_packs.clear();
        self.shader_picked.clear();
        self.shader_dir = None;
        self.shader_graphics = None;
        self.gfx = None;
        self.gfx_paths = None;
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
        self.load_graphics_state(&instance_root);
    }

    fn load_graphics_state(&mut self, instance_root: &Path) {
        let options = instance_root.join("options.txt");
        let graphics = instance_root.join("horizon-graphics.txt");
        match GfxSettings::read(&options, &graphics) {
            Ok(settings) => self.gfx = Some(settings),
            Err(error) => {
                self.gfx = None;
                self.status = Some(Status {
                    message: format!("Could not read graphics settings: {error}"),
                    kind: StatusKind::Error,
                });
            }
        }
        self.gfx_paths = Some((options, graphics));
    }

    /// Switches page. Graphics re-reads the files so values changed in-game are not stale.
    fn go_to(&mut self, page: Page) {
        if page == Page::Graphics && self.page != Page::Graphics {
            self.reload_gfx();
        }
        self.page = page;
    }

    fn reload_gfx(&mut self) {
        if let Some((options, graphics)) = self.gfx_paths.as_ref() {
            match GfxSettings::read(options, graphics) {
                Ok(settings) => self.gfx = Some(settings),
                Err(error) => {
                    self.gfx = None;
                    self.status = Some(Status {
                        message: format!("Could not reload graphics settings: {error}"),
                        kind: StatusKind::Error,
                    });
                }
            }
        }
    }

    /// Writes one setting to `options.txt` or `horizon-graphics.txt`. Reports failure in the status line.
    fn write_gfx(&mut self, in_options: bool, key: &str, value: &str) -> bool {
        let Some((options, graphics)) = self.gfx_paths.as_ref() else { return false; };
        let result = if in_options {
            write_setting(options, key, value)
        } else {
            write_setting(graphics, key, value)
                .and_then(|()| write_setting(graphics, "preset", "custom"))
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                self.status = Some(Status { message: format!("Could not save {key}: {error}"), kind: StatusKind::Error });
                false
            }
        }
    }

    /// Writes `pickedShaderPack:` in `horizon-graphics.txt`, leaving every other line untouched.
    fn set_picked_shader(&mut self, key: &str) {
        if game_is_running() {
            self.status = Some(Status { message: "Close Horizon before changing shader packs; the game reads this setting at start.".into(), kind: StatusKind::Error });
            return;
        }
        let Some(path) = self.shader_graphics.as_deref() else {
            self.status = Some(Status { message: "No Horizon instance is selected for shader packs.".into(), kind: StatusKind::Error });
            return;
        };
        match write_picked_shader(path, key) {
            Ok(()) => {
                self.shader_picked.clear();
                self.shader_picked.push_str(key);
                let message = if key.is_empty() {
                    "Shaders disabled. Vanilla rendering returns next time Horizon starts.".to_owned()
                } else {
                    let title = self.shader_packs.iter()
                        .find(|pack| pack.key == key || pack.file == key)
                        .map(|pack| pack.title.as_str())
                        .unwrap_or(key);
                    format!("Enabled {title}. It applies next time Horizon starts.")
                };
                self.status = Some(Status { message, kind: StatusKind::Success });
            }
            Err(error) => self.status = Some(Status { message: format!("Could not update pickedShaderPack: {error}"), kind: StatusKind::Error }),
        }
    }

    /// Copies a dropped `.zip` / `.hshader` / pack folder into `shaderpacks/` and enables it.
    fn install_shader_pack(&mut self, source: &Path) {
        if game_is_running() {
            self.status = Some(Status { message: "Close Horizon before installing shader packs.".into(), kind: StatusKind::Error });
            return;
        }
        let Some(dir) = self.shader_dir.as_deref() else {
            self.status = Some(Status { message: "No Horizon instance is selected for shader packs.".into(), kind: StatusKind::Error });
            return;
        };
        let Some(name) = source.file_name().and_then(|name| name.to_str()) else {
            self.status = Some(Status { message: "That path has no usable file name.".into(), kind: StatusKind::Error });
            return;
        };
        if !is_shader_pack_path(source) {
            self.status = Some(Status { message: format!("{name} is not a shader pack: expected a .zip, .hshader, or a folder with shaders/ or horizon-shaders.toml."), kind: StatusKind::Error });
            return;
        }
        let destination = dir.join(name);
        if destination.exists() {
            self.status = Some(Status { message: format!("{name} is already installed."), kind: StatusKind::Error });
            return;
        }
        let copied = if source.is_dir() {
            copy_dir_all(source, &destination)
        } else {
            fs::copy(source, &destination).map(|_| ()).map_err(|error| error.to_string())
        };
        if let Err(error) = copied {
            self.status = Some(Status { message: format!("Could not install {name}: {error}"), kind: StatusKind::Error });
            return;
        }
        self.load_shader_state();
        let key = self.shader_packs.iter().find(|pack| pack.file == name)
            .map(|pack| pack.key.clone())
            .unwrap_or_else(|| name.to_owned());
        self.set_picked_shader(&key);
        if self.status.as_ref().is_some_and(|status| status.kind == StatusKind::Error) {
            return;
        }
        self.status = Some(Status { message: format!("Installed and enabled {name}. It applies next time Horizon starts."), kind: StatusKind::Success });
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
                kind: StatusKind::Error,
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
                kind: StatusKind::Error,
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
                    kind: StatusKind::Success,
                });
            }
            Err(error) => {
                self.status = Some(Status {
                    message: format!("Could not load Horizon accounts: {error}"),
                    kind: StatusKind::Error,
                });
            }
        }
    }

    fn launch(&mut self, ctx: &Context) {
        let Some(version) = self.selected_version().cloned() else {
            self.status = Some(Status {
                message: "No prepared Horizon client was found. Prepare a client build with your installed Horizon setup first.".into(),
                kind: StatusKind::Error,
            });
            return;
        };
        if self.selected_resource().is_none() {
            self.status = Some(Status {
                message: "This client has no prepared resource set.".into(),
                kind: StatusKind::Error,
            });
            return;
        }
        let Some(account) = self.selected_account_id() else {
            self.status = Some(Status {
                message: "Choose an account first. Use Load accounts to read names from Horizon's sign-in helper.".into(),
                kind: StatusKind::Error,
            });
            return;
        };
        let Some(launcher) = self.official_launcher.as_deref() else {
            self.status = Some(Status {
                message:
                    "Could not find Horizon's sign-in helper to create a one-time game session."
                        .into(),
                kind: StatusKind::Error,
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
                    kind: StatusKind::Success,
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
                    kind: StatusKind::Error,
                });
            }
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
                    kind: StatusKind::Error,
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
                        kind: StatusKind::Success,
                    });
                }
            }
            Ok(None) if pending.started.elapsed() >= std::time::Duration::from_secs(8) => {
                self.status = Some(Status {
                    message: "Horizon is preparing the game…".into(),
                    kind: StatusKind::Success,
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
            Ok(None) => ctx.request_repaint_after(std::time::Duration::from_millis(250)),
            Err(error) => {
                self.status = Some(Status {
                    message: format!("Could not check Horizon startup: {error}"),
                    kind: StatusKind::Error,
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
        // Ctrl/Cmd + 1..6 jumps to a page.
        const PAGE_KEYS: [egui::Key; 6] = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
        ];
        let hotkey = ctx.input(|input| {
            if !input.modifiers.command {
                return None;
            }
            (0..Page::ALL.len())
                .find(|&n| input.key_pressed(PAGE_KEYS[n]))
                .map(|n| Page::ALL[n].0)
        });
        if let Some(page) = hotkey {
            self.go_to(page);
        }
        // Each page eases in (fade and a short rise) when it becomes the current one.
        let mut fade = 1.0_f32;
        for (page, _) in Page::ALL {
            let t = ctx.animate_bool_with_time(egui::Id::new(("page_fade", page as u8)), self.page == page, 0.25);
            if self.page == page {
                fade = 1.0 - (1.0 - t).powi(3);
            }
        }
        self.windowbar(ctx);
        self.sidebar(ctx);

        // No full-screen overlay: the sunset stays visible around a single dark panel.
        egui::CentralPanel::default()
            .frame(Frame::NONE.inner_margin(Margin::symmetric(32, 24)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let width = (ui.available_width() - 56.0).clamp(320.0, 900.0);
                    ui.add_space((1.0 - fade) * 16.0);
                    let alpha = (PANEL_OPACITY as f32 * (0.55 + 0.45 * fade)) as u8;
                    panel_card(ui, main_panel_frame(alpha), |ui| {
                        ui.set_width(width);
                        match self.page {
                            Page::Play => self.play_page(ui, ctx),
                            Page::Performance => self.performance_page(ui),
                            Page::Graphics => self.graphics_page(ui),
                            Page::Shaders => self.shaders_page(ui, ctx),
                            Page::Diagnostics => self.diagnostics_page(ui),
                            Page::Settings => self.settings_page(ui),
                        }
                    });
                });
            });
    }
}

pub(crate) fn run() -> eframe::Result<()> {
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

// Reading and writing Horizon's `key:value` settings files. Standard library only.

/// Leaf, transparency and culling settings of one instance.
/// Values and defaults follow `horizon-client-render` (`graphics.rs`, `options_file.rs`).
#[derive(Debug, PartialEq)]
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

#[cfg(test)]
mod gfx_tests {
    use super::*;

    #[test]
    fn missing_keys_use_horizon_defaults() {
        let g = GfxSettings::parse("", "");
        assert!(g.cutout_leaves && g.cave_culling);
        assert!(!g.improved_transparency && !g.dynamic_resolution);
        assert_eq!(g.leaf_culling, LeafCulling::Off);
        assert_eq!(g.entity_culling, CullingMode::Off);
        assert_eq!(g.gpu_culling, GpuCulling::Off);
        assert_eq!(g.target_fps, 60);
        assert_eq!(g.frame_pacing, FramePacing::Efficient);
    }

    #[test]
    fn reads_values_and_rejects_unknown_ones() {
        let g = GfxSettings::parse(
            "cutoutLeaves:false\nimprovedTransparency:true\n",
            "leafCulling:hollow\nentityCulling:async\nparticleCulling:banana\ngpuCulling:auto\ncaveCulling:false\ndynamicResolution:true\ntargetFps:999\nframePacing:precise\n",
        );
        assert!(!g.cutout_leaves && g.improved_transparency && !g.cave_culling && g.dynamic_resolution);
        assert_eq!(g.leaf_culling, LeafCulling::Hollow);
        assert_eq!(g.entity_culling, CullingMode::Asynchronous);
        assert_eq!(g.particle_culling, CullingMode::Off, "unknown value falls back to the default");
        assert_eq!(g.gpu_culling, GpuCulling::Auto);
        assert_eq!(g.target_fps, 240, "clamped like Horizon");
        assert_eq!(g.frame_pacing, FramePacing::Precise);
    }

    #[test]
    fn key_match_is_exact_not_a_prefix() {
        let text = "fxaa:true\ntaa:false\n";
        assert_eq!(read_setting(text, "taa").as_deref(), Some("false"));
        assert_eq!(read_setting(text, "xaa"), None);
        let out = set_setting_text(text, "taa", "true");
        assert_eq!(out, "fxaa:true\ntaa:true\n");
    }

    #[test]
    fn set_keeps_every_other_line_and_appends_when_missing() {
        let text = "preset:performance\nlodDistance:128\nleafCulling:off\nresolutionScale:80\n";
        let out = set_setting_text(text, "leafCulling", "hidden");
        assert_eq!(out, "preset:performance\nlodDistance:128\nleafCulling:hidden\nresolutionScale:80\n");
        let out = set_setting_text(text, "caveCulling", "true");
        assert!(out.ends_with("resolutionScale:80\ncaveCulling:true\n"));
        assert_eq!(out.lines().count(), 5);
    }

    #[test]
    fn write_setting_round_trips_on_disk_and_errors_when_missing() {
        let dir = std::env::temp_dir().join(format!("nebula-gfx-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let graphics = dir.join("horizon-graphics.txt");
        let options = dir.join("options.txt");
        fs::write(&graphics, "preset:performance\nleafCulling:off\npickedShaderPack:a.zip\n").unwrap();
        fs::write(&options, "fov:70\ncutoutLeaves:true\nmaxFps:120\n").unwrap();

        write_setting(&options, "cutoutLeaves", "false").unwrap();
        write_setting(&graphics, "leafCulling", "hidden").unwrap();
        write_setting(&graphics, "preset", "custom").unwrap();
        let g = GfxSettings::read(&options, &graphics).expect("test files should be readable");
        assert!(!g.cutout_leaves);
        assert_eq!(g.leaf_culling, LeafCulling::Hidden);
        let kept = fs::read_to_string(&graphics).unwrap();
        assert!(kept.contains("pickedShaderPack:a.zip") && kept.contains("preset:custom"));
        assert!(fs::read_to_string(&options).unwrap().contains("fov:70\n"));
        assert!(!dir.join("options.txt.tmp").exists(), "temp file renamed away");

        let missing = dir.join("nope.txt");
        assert!(write_setting(&missing, "a", "b").is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn presets_change_only_their_own_keys() {
        let mut g = GfxSettings::parse("cutoutLeaves:true\n", "dynamicResolution:true\ntargetFps:90\n");
        let changes = g.apply_smooth_same_look();
        assert_eq!(changes.len(), 6);
        assert!(changes.iter().all(|(in_options, _, _)| !in_options));
        assert!(g.dynamic_resolution && g.target_fps == 90 && g.cutout_leaves, "untouched");
        let keys: Vec<_> = changes.iter().map(|c| c.1).collect();
        assert!(!keys.contains(&"cutoutLeaves") && !keys.contains(&"dynamicResolution"));

        let changes = g.apply_plain_rendering();
        assert!(!g.dynamic_resolution && g.leaf_culling == LeafCulling::Off && g.gpu_culling == GpuCulling::Off);
        assert!(changes.iter().any(|c| c.1 == "dynamicResolution" && c.2 == "false"));
        assert!(g.cutout_leaves, "plain rendering never touches leaf transparency");
    }

    #[test]
    fn popup_opens_below_when_it_fits() {
        assert_eq!(plan_popup(100.0, 142.0, 0.0, 780.0, 300.0), PopupPlan { below: true, height: 300.0 });
    }

    #[test]
    fn popup_flips_above_when_there_is_more_room_there() {
        assert_eq!(plan_popup(650.0, 692.0, 0.0, 780.0, 300.0), PopupPlan { below: false, height: 300.0 });
    }

    #[test]
    fn popup_scrolls_when_neither_side_fits_it_whole() {
        let plan = plan_popup(190.0, 232.0, 0.0, 400.0, 300.0);
        assert!(!plan.below);
        assert!((plan.height - 172.0).abs() < 0.01);
    }

    #[test]
    fn popup_never_collapses() {
        assert_eq!(plan_popup(0.0, 42.0, 0.0, 50.0, 300.0).height, 120.0);
        assert_eq!(plan_popup(0.0, 42.0, 0.0, 50.0, 80.0).height, 80.0);
    }

    #[test]
    fn highlight_steps_without_wrapping() {
        assert_eq!(step_highlight(None, 4, 1), Some(0));
        assert_eq!(step_highlight(None, 4, -1), Some(3));
        assert_eq!(step_highlight(Some(1), 4, 1), Some(2));
        assert_eq!(step_highlight(Some(3), 4, 1), Some(3));
        assert_eq!(step_highlight(Some(0), 4, -1), Some(0));
        assert_eq!(step_highlight(Some(9), 4, 1), Some(3));
        assert_eq!(step_highlight(Some(1), 0, 1), None);
    }

    #[test]
    fn segments_fit_their_row() {
        assert_eq!(segment_width(30.0, 3, 600.0), 64.0);
        assert_eq!(segment_width(100.0, 5, 800.0), 128.0);
        assert_eq!(segment_width(100.0, 5, 500.0), 100.0);
        assert_eq!(segment_width(100.0, 5, 100.0), 40.0);
    }

    #[test]
    fn easing_is_clamped() {
        assert_eq!(ease_out_cubic(-1.0), 0.0);
        assert_eq!(ease_out_cubic(2.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    /// WCAG contrast ratio of two opaque colours.
    fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
        let lum = |c: [u8; 3]| {
            let f = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])
        };
        let (a, b) = (lum(a), lum(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_stays_readable_over_the_brightest_sunset_glow() {
        // Brightest sky behind the panel: the lowest sky band lit by the sun's glow.
        let sky = [209.0_f32, 95.0, 108.0];
        let k = PANEL_OPACITY as f32 / 255.0;
        let panel = [12.0_f32, 9.0, 27.0];
        let bg: [u8; 3] = std::array::from_fn(|i| {
            (panel[i] * k + sky[i] * (1.0 - k)).round() as u8
        });
        for (name, color) in [
            ("THEME.text", THEME.text),
            ("THEME.muted", THEME.muted),
            ("THEME.primary_bright", THEME.primary_bright),
            ("THEME.primary_bright", THEME.primary_bright),
            ("THEME.danger", THEME.danger),
            ("THEME.success", THEME.success),
        ] {
            let ratio = contrast([color.r(), color.g(), color.b()], bg);
            assert!(ratio >= 4.5, "{name} only {ratio:.2}:1");
        }
    }
}
