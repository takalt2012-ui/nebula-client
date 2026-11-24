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

#[cfg(test)]
mod tests {
    use super::ensure_builtin_shader_pack;
    use std::{fs, path::PathBuf};

    fn test_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "nebula-golden-hour-{}",
            std::process::id()
        ))
    }

    #[test]
    fn provisions_a_complete_native_pack_and_preserves_user_edits() {
        let root = test_directory();
        let _ = fs::remove_dir_all(&root);

        let result = ensure_builtin_shader_pack(&root);
        assert!(result.is_ok(), "provisioning failed: {result:?}");

        let pack = root.join("Nebula Golden Hour");
        let expected = [
            "horizon-shaders.toml",
            "common.wgsl",
            "sky.wgsl",
            "post/glow.wgsl",
            "post/grade.wgsl",
        ];
        for relative in expected {
            let path = pack.join(relative);
            assert!(path.is_file(), "missing bundled shader file: {}", path.display());
            let contents = fs::read_to_string(&path);
            assert!(contents.is_ok(), "could not read {}", path.display());
            assert!(!contents.unwrap_or_default().trim().is_empty());
        }

        let manifest = fs::read_to_string(pack.join("horizon-shaders.toml"));
        assert!(manifest.is_ok());
        assert!(manifest.unwrap_or_default().contains("api = \"0.1\""));

        let custom_sky = "user-customized shader contents\n";
        let sky_path = pack.join("sky.wgsl");
        let write_result = fs::write(&sky_path, custom_sky);
        assert!(write_result.is_ok());

        let second_run = ensure_builtin_shader_pack(&root);
        assert!(second_run.is_ok(), "second provisioning failed: {second_run:?}");
        let preserved = fs::read_to_string(&sky_path);
        assert!(matches!(preserved, Ok(contents) if contents == custom_sky));

        let _ = fs::remove_dir_all(&root);
    }
}
