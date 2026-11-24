use super::{
    AccountInfo, AccountList, Capabilities, HorizonVersion, LauncherSettings, ResourceSet,
    ShaderPackEntry,
};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::SystemTime;

pub(super) fn discover_versions() -> Vec<HorizonVersion> {
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

pub(super) fn spawn_game(
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

pub(super) fn selected_instance(launcher: &Path, tag: &str) -> Result<String, String> {
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
pub(super) fn launch_argv(
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

pub(super) fn game_is_running() -> bool {
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

pub(super) fn command_exists(command: &str) -> bool {
    command_in_path(command).is_some()
}

fn command_in_path(command: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|folder| folder.join(command))
            .find(|path| path.is_file())
    })
}

pub(super) fn discover_official_launcher() -> Option<PathBuf> {
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

pub(super) fn accounts_from_official_launcher(launcher: &Path) -> Result<Vec<AccountInfo>, String> {
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

pub(super) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

pub(super) fn horizon_data_root() -> PathBuf {
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

pub(super) fn cache_dir() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".cache")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nebula-client")
}

pub(super) fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// The current `pickedShaderPack:` value; empty when unset or shaders are off.
pub(super) fn read_picked_shader(path: &Path) -> String {
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
pub(super) fn write_picked_shader(path: &Path, value: &str) -> Result<(), String> {
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

/// Ensures the bundled native shader is available in the selected instance.
///
/// The shader sources are embedded at compile time so packaged launchers do not depend on
/// the repository checkout being present on the user's machine. Existing user edits are kept;
/// only missing files are provisioned.
pub(super) fn ensure_builtin_shader_pack(shaderpacks: &Path) -> Result<(), String> {
    let destination = shaderpacks.join("Nebula Golden Hour");
    let files: [(&str, &str); 5] = [
        (
            "horizon-shaders.toml",
            include_str!("../../assets/nebula-golden-hour/horizon-shaders.toml"),
        ),
        (
            "common.wgsl",
            include_str!("../../assets/nebula-golden-hour/common.wgsl"),
        ),
        (
            "sky.wgsl",
            include_str!("../../assets/nebula-golden-hour/sky.wgsl"),
        ),
        (
            "post/glow.wgsl",
            include_str!("../../assets/nebula-golden-hour/post/glow.wgsl"),
        ),
        (
            "post/grade.wgsl",
            include_str!("../../assets/nebula-golden-hour/post/grade.wgsl"),
        ),
    ];

    for (relative_path, source) in files {
        let path = destination.join(relative_path);
        if path.is_file() {
            continue;
        }
        let parent = path
            .parent()
            .ok_or_else(|| format!("invalid bundled shader path: {}", path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        fs::write(&path, source)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    }

    Ok(())
}

/// Scans `shaderpacks/` for Horizon-native and classic `shaders/` packs.
pub(super) fn scan_shader_packs(dir: &Path) -> Vec<ShaderPackEntry> {
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
    // Keep the bundled Nebula pack immediately visible, then native and classic packs.
    packs.sort_by(|a, b| {
        let a_builtin = a.file != "Nebula Golden Hour";
        let b_builtin = b.file != "Nebula Golden Hour";
        (a_builtin, !a.native, a.title.to_lowercase())
            .cmp(&(b_builtin, !b.native, b.title.to_lowercase()))
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

pub(super) fn is_shader_pack_path(path: &Path) -> bool {
    if path.is_dir() {
        return path.join("shaders").is_dir() || path.join("horizon-shaders.toml").is_file();
    }
    path.is_file()
        && path.extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("hshader")
        })
}

pub(super) fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), String> {
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

pub(super) fn load_settings() -> Result<LauncherSettings, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(LauncherSettings::default());
    }
    let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

pub(super) fn save_settings(settings: &LauncherSettings) -> Result<(), String> {
    let folder = config_dir();
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let text = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    let temporary = folder.join("settings.json.tmp");
    fs::write(&temporary, text).map_err(|error| error.to_string())?;
    fs::rename(&temporary, settings_path()).map_err(|error| error.to_string())
}

pub(super) fn yes_no(value: bool) -> &'static str {
    if value {
        "available"
    } else {
        "not installed"
    }
}

pub(super) fn print_diagnostics() {
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

