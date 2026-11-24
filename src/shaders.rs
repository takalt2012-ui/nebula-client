//! Bundled Horizon-native shader assets and per-instance provisioning.

use std::fs;
use std::path::Path;

/// Installs the built-in shader into an instance without depending on a source checkout.
///
/// Files are embedded at compile time. Existing user edits are preserved; an incomplete pack
/// directory is repaired by writing only the missing files.
pub(crate) fn ensure_builtin_shader_pack(shaderpacks: &Path) -> Result<(), String> {
    let destination = shaderpacks.join("Nebula Golden Hour");
    let files: [(&str, &str); 5] = [
        (
            "horizon-shaders.toml",
            include_str!("../assets/nebula-golden-hour/horizon-shaders.toml"),
        ),
        (
            "common.wgsl",
            include_str!("../assets/nebula-golden-hour/common.wgsl"),
        ),
        (
            "sky.wgsl",
            include_str!("../assets/nebula-golden-hour/sky.wgsl"),
        ),
        (
            "post/glow.wgsl",
            include_str!("../assets/nebula-golden-hour/post/glow.wgsl"),
        ),
        (
            "post/grade.wgsl",
            include_str!("../assets/nebula-golden-hour/post/grade.wgsl"),
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
