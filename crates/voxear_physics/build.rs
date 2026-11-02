//! Builds Jolt Physics from its sources in `jolt/`, with the shim in
//! `shim/`, as one C++ static library. It is built optimised even in debug
//! builds, as physics at -O0 is too slow to play with, and without Jolt's
//! profiler, debug renderer and asserts.

use std::path::{Path, PathBuf};

fn main() {
    let jolt = Path::new("jolt");
    let mut sources = Vec::new();
    collect(&jolt.join("Jolt"), &mut sources);
    sources.push(PathBuf::from("shim/voxear_jolt.cpp"));
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(jolt)
        .files(&sources)
        .opt_level(2)
        .debug(false)
        .warnings(false)
        .define("NDEBUG", None)
        .flag_if_supported("-fno-strict-aliasing")
        .flag_if_supported("-Wno-everything");
    build.compile("voxear_jolt");
    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=jolt/Jolt");
}

/// Every `.cpp` under `dir`; the Metal backend's `.mm` files are not C++.
fn collect(dir: &Path, sources: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, sources);
        } else if path.extension().is_some_and(|e| e == "cpp") {
            sources.push(path);
        }
    }
}
