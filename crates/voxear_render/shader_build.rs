// Shared by build.rs and the runtime shader reloader (src/hot_reload.rs), so
// both compile shaders the same way.

/// Shader files that are only included by others, never compiled alone.
pub const SHADER_MODULES: &[&str] =
    &["atmosphere", "common", "culling", "custom", "ddgi_sample", "glow", "impostor", "lava", "lights", "occlusion", "particle_shared", "seasons", "shading", "terrain_ground", "trace_software", "ui_custom", "ui_effects", "volume", "wind"];

/// Shader files that draw materials, and so call a material's custom
/// shader, and the UI's, which calls a project's UI shaders: compiled again
/// with a project's custom shaders.
#[allow(dead_code)]
pub const CUSTOM_SHADER_MODULES: &[&str] = &["forward", "meshlets", "shadow", "meshlet_shadow", "preview", "glass", "ui"];

/// `slangc` arguments besides the input file, include path and output.
pub const SLANG_ARGS: &[&str] = &[
    "-target",
    "spirv",
    "-profile",
    "spirv_1_6",
    "-O2",
    "-matrix-layout-column-major",
    "-fvk-use-scalar-layout",
    "-fvk-use-entrypoint-name",
];
