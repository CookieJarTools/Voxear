//! The renderer keeps the pipelines it made on disk, and a second start
//! takes them from there instead of compiling them again. The driver's own
//! cache is turned off here, so it cannot hide how long compiling takes.
//! Skips itself (with a note) on machines without a usable Vulkan device.
//!
//! With `VOXEAR_TIME_ONE_AT_A_TIME=1` it also times a start that makes its
//! pipelines one at a time and keeps none, as every start did before.

use std::time::Instant;

use voxear_render::{Renderer, RendererSettings};

/// A renderer made, how long that took, and the time it reports for
/// making its pipelines is in the log.
fn started() -> Option<(Renderer, f32)> {
    let started = Instant::now();
    match Renderer::new_offscreen(64, 64, &RendererSettings::default()) {
        Ok(renderer) => Some((renderer, started.elapsed().as_secs_f32())),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

#[test]
fn a_second_start_takes_the_pipelines_the_first_made() {
    let folder = std::env::temp_dir().join(format!("voxear-pipelines-{}", std::process::id()));
    // The only test in this program, so nothing else reads these while they change.
    unsafe {
        std::env::set_var("__GL_SHADER_DISK_CACHE", "0");
        std::env::set_var("MESA_SHADER_CACHE_DISABLE", "true");
    }
    if std::env::var("VOXEAR_TIME_ONE_AT_A_TIME").is_ok_and(|set| set == "1") {
        unsafe {
            std::env::set_var("VOXEAR_PIPELINE_CACHE", "0");
            std::env::set_var("VOXEAR_PIPELINE_THREADS", "1");
        }
        let Some((renderer, took)) = started() else { return };
        drop(renderer);
        eprintln!("one at a time, keeping none: {took:.1} s");
        unsafe { std::env::remove_var("VOXEAR_PIPELINE_THREADS") };
    }
    unsafe { std::env::set_var("VOXEAR_PIPELINE_CACHE", &folder) };
    let Some((renderer, cold)) = started() else { return };
    drop(renderer);
    let kept: Vec<_> =
        std::fs::read_dir(&folder).map(|entries| entries.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    let (Some((renderer, warm)), sizes) = (started(), kept.iter().map(|path| path.metadata().map_or(0, |m| m.len())))
    else {
        return;
    };
    let size: u64 = sizes.sum();
    drop(renderer);
    std::fs::remove_dir_all(&folder).ok();
    eprintln!("all at once: {cold:.1} s; from what that kept: {warm:.1} s; kept {:.1} MB", size as f64 / 1e6);
    assert_eq!(kept.len(), 1, "one file for this build's shaders on this GPU: {kept:?}");
    assert!(size > 0, "the file holds the pipelines");
    assert!(warm < cold, "taking the pipelines from the file is quicker than making them ({warm:.1} s, {cold:.1} s)");
}
