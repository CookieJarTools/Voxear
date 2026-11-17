// Shared by build.rs and the runtime shader reloader (src/hot_reload.rs), so
// both compile shaders the same way.

/// Shader files that are only included by others, never compiled alone.
pub const SHADER_MODULES: &[&str] =
    &["atmosphere", "block_quads", "clouds", "common", "culling", "custom", "ddgi_sample", "glow", "impostor", "lava", "lights", "occlusion", "particle_custom", "particle_motion", "particle_shared", "reflection_probes", "seasons", "shading", "sound_rays", "look_custom", "terrain_ground", "trace_software", "ui_custom", "ui_effects", "volume", "wind", "light_bake", "lightmaps", "virtual_shadows", "shader_pack"];

/// Shader files that draw materials, and so call a material's custom
/// shader, the UI's, which calls a project's UI shaders, and the particles'
/// drawing and step, which call its particle graphs: compiled again with a
/// project's custom shaders.
#[allow(dead_code)]
pub const CUSTOM_SHADER_MODULES: &[&str] =
    &["forward", "meshlets", "shadow", "meshlet_shadow", "preview", "glass", "ui", "post", "particles", "particle_sim"];

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
