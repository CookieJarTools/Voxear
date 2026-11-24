//! Compiles every Slang shader in `shaders/` that has entry points to
//! SPIR-V with `slangc` (the `shader-slang` package on Arch, the Vulkan SDK
//! on Windows). Set `SLANGC` to use a specific compiler.
//!
//! The shaders are compiled side by side, as many at once as the build may
//! run, and what they make is kept in `target/build-cache` by the shaders,
//! the arguments and the compiler, so a new build folder copies it.

use std::{env, path::PathBuf, process::Command};

include!("shader_build.rs");
include!("../build_cache.rs");

fn main() {
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-changed=fsr3");
    println!("cargo:rerun-if-changed=shader_build.rs");
    println!("cargo:rerun-if-changed=../build_cache.rs");
    println!("cargo:rerun-if-env-changed=SLANGC");
    let slangc = env::var("SLANGC").unwrap_or_else(|_| "slangc".into());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut shaders = Vec::new();
    for entry in std::fs::read_dir("shaders").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("slang") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        if !SHADER_MODULES.contains(&stem.as_str()) {
            shaders.push((path, stem));
        }
    }
    shaders.sort();

    let mut key = BuildKey::new(&SLANG_ARGS.join(" "));
    key.add(&std::fs::read("build.rs").unwrap());
    key.add(&std::fs::read("shader_build.rs").unwrap());
    key.add_folder(std::path::Path::new("shaders"));
    key.add_folder(std::path::Path::new("fsr3"));
    key.add_version(&slangc);
    let name = key.name("shaders");
    if take_kept(&name, &out) {
        return;
    }

    let mut made: Vec<PathBuf> = shaders.iter().map(|(_, stem)| PathBuf::from(format!("{stem}.spv"))).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let at_once =
        env::var("NUM_JOBS").ok().and_then(|jobs| jobs.parse().ok()).unwrap_or(4).clamp(1, shaders.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..at_once {
            scope.spawn(|| {
                while let Some((path, stem)) = shaders.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                    compile(&slangc, path, &out.join(format!("{stem}.spv")));
                }
            });
        }
    });
    made.extend(fsr3(&slangc, &out));
    keep(&name, &out, &made, 8);
}

fn compile(slangc: &str, path: &std::path::Path, target: &std::path::Path) {
    let output = Command::new(slangc)
        .arg(path)
        .args(SLANG_ARGS)
        .args(["-I", "shaders", "-o"])
        .arg(target)
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

/// The FSR 3.1 upscaler's passes, from AMD's HLSL in `fsr3/amd`, as Voxear
/// uses them: HDR colour, motion vectors at the drawn size, reverse-Z depth.
/// Each is compiled to SPIR-V with its resources' bindings kept apart by kind
/// (constant buffers from 0, textures from 16, storage images from 48,
/// samplers from 80), and its reflection turned into the table `fsr3` binds
/// from: Voxear's permutation table, one entry a pass. Returns the files it
/// made.
fn fsr3(slangc: &str, out: &std::path::Path) -> Vec<PathBuf> {
    let mut made = vec![PathBuf::from("fsr3_passes.rs")];
    const PASSES: [&str; 8] = [
        "prepare_inputs",
        "luma_pyramid",
        "shading_change_pyramid",
        "shading_change",
        "prepare_reactivity",
        "luma_instability",
        "accumulate",
        // The reactive mask, from the scene before and after what is drawn over it.
        "autogen_reactive",
    ];
    let root = std::path::Path::new("fsr3/amd");
    let mut table = String::from("// Made by build.rs from slangc's reflection of AMD's FSR 3.1 passes.\n");
    table.push_str("pub(crate) const FSR3_PASSES: [Fsr3PassInfo; 8] = [\n");
    for pass in PASSES {
        let source = root.join(format!("upscalers/fsr3/internal/shaders/ffx_fsr3upscaler_{pass}_pass.hlsl"));
        let spirv = out.join(format!("fsr3_{pass}.spv"));
        let reflection = out.join(format!("fsr3_{pass}.json"));
        made.push(PathBuf::from(format!("fsr3_{pass}.spv")));
        made.push(PathBuf::from(format!("fsr3_{pass}.json")));
        let output = Command::new(slangc)
            .arg(&source)
            .args(["-entry", "CS", "-stage", "compute", "-target", "spirv", "-profile", "sm_6_5"])
            .arg("-I")
            .arg(root.join("upscalers/fsr3/include/gpu"))
            .arg("-I")
            .arg(root.join("api/internal/gpu"))
            .args(["-DFFX_GPU=1", "-DFFX_HLSL=1", "-DFFX_HALF=0", "-DFFX_HLSL_SM=62"])
            .args([
                "-DFFX_FSR3UPSCALER_OPTION_HDR_COLOR_INPUT=1",
                "-DFFX_FSR3UPSCALER_OPTION_LOW_RESOLUTION_MOTION_VECTORS=1",
                "-DFFX_FSR3UPSCALER_OPTION_JITTERED_MOTION_VECTORS=0",
                "-DFFX_FSR3UPSCALER_OPTION_INVERTED_DEPTH=1",
                "-DFFX_FSR3UPSCALER_OPTION_APPLY_SHARPENING=0",
                "-DFFX_FSR3UPSCALER_OPTION_REPROJECT_USE_LANCZOS_TYPE=0",
            ])
            .args(["-fvk-b-shift", "0", "0", "-fvk-t-shift", "16", "0", "-fvk-u-shift", "48", "0"])
            .args(["-fvk-s-shift", "80", "0", "-o"])
            .arg(&spirv)
            .arg("-reflection-json")
            .arg(&reflection)
            .output()
            .unwrap_or_else(|e| panic!("could not run `{slangc}` ({e})"));
        assert!(
            output.status.success(),
            "failed to compile {}:\n{}",
            source.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&reflection).unwrap()).unwrap();
        let size = &json["entryPoints"][0]["threadGroupSize"];
        let group: Vec<u64> = (0..3).map(|i| size[i].as_u64().unwrap_or(1)).collect();
        table.push_str(&format!(
            "    Fsr3PassInfo {{ name: \"{pass}\", spirv: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/fsr3_{pass}.spv\")), \
             group: [{}, {}, {}], bindings: &[\n",
            group[0], group[1], group[2]
        ));
        for parameter in json["parameters"].as_array().unwrap() {
            let name = parameter["name"].as_str().unwrap();
            let binding = parameter["binding"]["index"].as_u64().unwrap();
            let kind = match parameter["binding"]["kind"].as_str().unwrap() {
                "constantBuffer" => "Uniform",
                "samplerState" => "Sampler",
                "shaderResource" => "Texture",
                "unorderedAccess" => "Storage",
                other => panic!("{pass}: a binding of a kind FSR's host does not bind: {other}"),
            };
            table.push_str(&format!("        (\"{name}\", Fsr3Binding::{kind}, {binding}),\n"));
        }
        table.push_str("    ] },\n");
    }
    table.push_str("];\n");
    std::fs::write(out.join("fsr3_passes.rs"), table).unwrap();
    made
}
