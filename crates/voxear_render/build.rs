//! Compiles GLSL to SPIR-V with `glslc` (shipped with the Vulkan SDK, or the
//! `shaderc` package on Linux). Set `GLSLC` to use a specific compiler.

use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-env-changed=GLSLC");
    let glslc = env::var("GLSLC").unwrap_or_else(|_| "glslc".into());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    for entry in std::fs::read_dir("shaders").unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
        if !matches!(ext, "vert" | "frag" | "comp") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let target = out.join(format!("{name}.spv"));
        let status = Command::new(&glslc)
            .args(["--target-env=vulkan1.3", "-O", "-o"])
            .arg(&target)
            .arg(&path)
            .status()
            .unwrap_or_else(|e| {
                panic!("could not run `{glslc}` ({e}); install the Vulkan SDK, or `shaderc` on Linux")
            });
        assert!(status.success(), "failed to compile {}", path.display());
    }
}
