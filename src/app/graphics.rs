use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum LeafCulling {
    #[default]
    Off,
    Hidden,
    Hollow,
    Solid,
    SolidAggressive,
}

impl LeafCulling {
    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "off" => Some(Self::Off),
            "hidden" => Some(Self::Hidden),
            "hollow" => Some(Self::Hollow),
            "solid" => Some(Self::Solid),
            "solid_aggressive" => Some(Self::SolidAggressive),
            _ => None,
        }
    }

    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Hidden => "hidden",
            Self::Hollow => "hollow",
            Self::Solid => "solid",
            Self::SolidAggressive => "solid_aggressive",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum CullingMode {
    #[default]
    Off,
    Synchronous,
    Asynchronous,
}

impl CullingMode {
    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "off" => Some(Self::Off),
            "sync" => Some(Self::Synchronous),
            "async" => Some(Self::Asynchronous),
            _ => None,
        }
    }

    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Synchronous => "sync",
            Self::Asynchronous => "async",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GpuCulling {
    #[default]
    Off,
    On,
    Auto,
}

impl GpuCulling {
    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum FramePacing {
    #[default]
    Efficient,
    Precise,
}

impl FramePacing {
    pub(super) fn from_key(key: &str) -> Option<Self> {
        match key {
            "efficient" => Some(Self::Efficient),
            "precise" => Some(Self::Precise),
            _ => None,
        }
    }

    pub(super) const fn key(self) -> &'static str {
        match self {
            Self::Efficient => "efficient",
            Self::Precise => "precise",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GfxSettings {
    /// `options.txt` `cutoutLeaves`: see-through gaps in leaves.
    pub(super) cutout_leaves: bool,
    /// `options.txt` `improvedTransparency`.
    pub(super) improved_transparency: bool,
    /// `horizon-graphics.txt` `leafCulling`: off, hidden, solid, solid_aggressive, hollow.
    pub(super) leaf_culling: LeafCulling,
    /// `entityCulling` and `particleCulling`: off, sync, async.
    pub(super) entity_culling: CullingMode,
    pub(super) particle_culling: CullingMode,
    /// `gpuCulling`: off, on, auto.
    pub(super) gpu_culling: GpuCulling,
    pub(super) cave_culling: bool,
    pub(super) dynamic_resolution: bool,
    /// 30..=240.
    pub(super) target_fps: u32,
    /// `framePacing`: efficient or precise.
    pub(super) frame_pacing: FramePacing,
}

pub(super) type GfxChange = (bool, &'static str, String);

fn read_settings_text(path: &Path) -> Result<String, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
    }
}

impl GfxSettings {
    pub(super) fn read(options: &Path, graphics: &Path) -> Result<Self, String> {
        let options_text = read_settings_text(options)?;
        let graphics_text = read_settings_text(graphics)?;
        Ok(Self::parse(&options_text, &graphics_text))
    }

    pub(super) fn parse(options_text: &str, graphics_text: &str) -> Self {
        let flag = |text: &str, key: &str, default: bool| match read_setting(text, key).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let word = |key: &str, allowed: &[&str], default: &str| {
            read_setting(graphics_text, key)
                .filter(|value| allowed.contains(&value.as_str()))
                .unwrap_or_else(|| default.to_owned())
        };
        Self {
            cutout_leaves: flag(options_text, "cutoutLeaves", true),
            improved_transparency: flag(options_text, "improvedTransparency", false),
            leaf_culling: LeafCulling::from_key(&word(
                "leafCulling",
                &["off", "hidden", "solid", "solid_aggressive", "hollow"],
                "off",
            ))
            .unwrap_or_default(),
            entity_culling: CullingMode::from_key(&word("entityCulling", &["off", "sync", "async"], "off"))
                .unwrap_or_default(),
            particle_culling: CullingMode::from_key(&word("particleCulling", &["off", "sync", "async"], "off"))
                .unwrap_or_default(),
            gpu_culling: GpuCulling::from_key(&word("gpuCulling", &["off", "on", "auto"], "off"))
                .unwrap_or_default(),
            cave_culling: flag(graphics_text, "caveCulling", true),
            dynamic_resolution: flag(graphics_text, "dynamicResolution", false),
            target_fps: read_setting(graphics_text, "targetFps")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(60)
                .clamp(30, 240),
            frame_pacing: FramePacing::from_key(&word("framePacing", &["efficient", "precise"], "efficient"))
                .unwrap_or_default(),
        }
    }

    /// Culling that skips drawing what cannot be seen. Meant to leave the picture unchanged;
    /// the leaf mode "hidden" keeps a crown looking the same from outside.
    pub(super) fn apply_smooth_same_look(&mut self) -> Vec<GfxChange> {
        self.cave_culling = true;
        self.entity_culling = CullingMode::Asynchronous;
        self.particle_culling = CullingMode::Asynchronous;
        self.gpu_culling = GpuCulling::Auto;
        self.leaf_culling = LeafCulling::Hidden;
        self.frame_pacing = FramePacing::Efficient;
        vec![
            (false, "caveCulling", "true".into()),
            (false, "entityCulling", CullingMode::Asynchronous.key().to_owned()),
            (false, "particleCulling", CullingMode::Asynchronous.key().to_owned()),
            (false, "gpuCulling", GpuCulling::Auto.key().to_owned()),
            (false, "leafCulling", LeafCulling::Hidden.key().to_owned()),
            (false, "framePacing", FramePacing::Efficient.key().to_owned()),
        ]
    }

    /// Back to Horizon's plain defaults for every culling option and dynamic resolution.
    pub(super) fn apply_plain_rendering(&mut self) -> Vec<GfxChange> {
        self.cave_culling = true;
        self.entity_culling = CullingMode::Off;
        self.particle_culling = CullingMode::Off;
        self.gpu_culling = GpuCulling::Off;
        self.leaf_culling = LeafCulling::Off;
        self.dynamic_resolution = false;
        vec![
            (false, "caveCulling", "true".into()),
            (false, "entityCulling", CullingMode::Off.key().to_owned()),
            (false, "particleCulling", CullingMode::Off.key().to_owned()),
            (false, "gpuCulling", GpuCulling::Off.key().to_owned()),
            (false, "leafCulling", LeafCulling::Off.key().to_owned()),
            (false, "dynamicResolution", "false".into()),
        ]
    }
}

/// The value of `key:` in a settings file's text, trimmed.
pub(super) fn read_setting(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    text.lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()).map(|value| value.trim().to_owned()))
}

/// Replaces every `key:` line, or appends one; all other lines are kept as they are.
pub(super) fn set_setting_text(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}:");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut replaced = false;
    for line in &mut lines {
        if line.starts_with(prefix.as_str()) {
            *line = format!("{key}:{value}");
            replaced = true;
        }
    }
    if !replaced {
        lines.push(format!("{key}:{value}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Writes one setting to an existing settings file through a temporary file and a rename.
pub(super) fn write_setting(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let out = set_setting_text(&text, key, value);
    let temporary = path.with_extension("txt.tmp");
    fs::write(&temporary, out).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}
// </settings-logic>

