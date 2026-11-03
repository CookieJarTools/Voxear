// Shared by build.rs and the runtime shader reloader (src/hot_reload.rs), so
// both compile shaders the same way.

/// Shader files that are only included by others, never compiled alone.
pub const SHADER_MODULES: &[&str] = &["atmosphere", "common", "culling", "lights", "occlusion", "shading"];

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
