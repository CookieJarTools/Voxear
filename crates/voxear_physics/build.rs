//! Builds Jolt Physics from its sources in `jolt/`, with the shim in
//! `shim/`, as one C++ static library. It is built optimised even in debug
//! builds, as physics at -O0 is too slow to play with, and without Jolt's
//! profiler, debug renderer and asserts.
//!
//! Building it takes minutes, so the library is kept in `target/build-cache`
//! by its sources, compiler and target, and a new build folder copies it.

use std::path::{Path, PathBuf};

include!("../build_cache.rs");

fn main() {
    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=jolt/Jolt");
    println!("cargo:rerun-if-changed=../build_cache.rs");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let target = std::env::var("TARGET").unwrap();
    let jolt = Path::new("jolt");
    let mut key = BuildKey::new(&target);
    key.add(&std::fs::read("build.rs").unwrap());
    key.add_folder(&jolt.join("Jolt"));
    key.add_folder(Path::new("shim"));
    let mut build = cc::Build::new();
    build.cpp(true).cargo_metadata(false);
    key.add_version(&build.get_compiler().path().to_string_lossy());
    let name = key.name("jolt");
    let library = if target.contains("msvc") { "voxear_jolt.lib" } else { "libvoxear_jolt.a" };
    if !take_kept(&name, &out) {
        compile(jolt, build);
        keep(&name, &out, &[PathBuf::from(library)], 4);
    }
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=voxear_jolt");
    if target.contains("apple") || target.contains("freebsd") {
        println!("cargo:rustc-link-lib=c++");
    } else if !target.contains("msvc") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}

fn compile(jolt: &Path, mut build: cc::Build) {
    let mut sources = Vec::new();
    collect(&jolt.join("Jolt"), &mut sources);
    sources.push(PathBuf::from("shim/voxear_jolt.cpp"));
    build
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
