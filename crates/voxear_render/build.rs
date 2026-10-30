//! Compiles every Slang shader in `shaders/` that has entry points to
//! SPIR-V with `slangc` (the `shader-slang` package on Arch, the Vulkan SDK
//! on Windows). Set `SLANGC` to use a specific compiler.

use std::{env, path::PathBuf, process::Command};

include!("shader_build.rs");

fn main() {
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-changed=shader_build.rs");
    println!("cargo:rerun-if-env-changed=SLANGC");
    let slangc = env::var("SLANGC").unwrap_or_else(|_| "slangc".into());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    for entry in std::fs::read_dir("shaders").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("slang") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        if SHADER_MODULES.contains(&stem.as_str()) {
            continue;
        }
        let target = out.join(format!("{stem}.spv"));
        let output = Command::new(&slangc)
            .arg(&path)
            .args(SLANG_ARGS)
            .args(["-I", "shaders", "-o"])
            .arg(&target)
            .output()
            .unwrap_or_else(|e| {
                panic!("could not run `{slangc}` ({e}); install Slang: `shader-slang` on Arch, or the Vulkan SDK")
            });
        assert!(
            output.status.success(),
            "failed to compile {}:\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
