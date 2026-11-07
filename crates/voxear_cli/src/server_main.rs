//! `voxear-server`: serves a game `vox build` built for a dedicated server,
//! from the `Data` folder beside it. A server build copies it in under the
//! game's name. It never opens a window or touches a GPU, so it runs on a
//! machine with neither. It is the Dedicated Server part of an install.
//!
//! `--port <n>` picks the port it listens on, the project's otherwise, and
//! `--frames <n>` stops after that many ticks, for checking a build.

use anyhow::{Context, Result};
use voxear_engine::{
    net::NetStart,
    player::{self, PlayerArgs},
};

/// Counts Rust's heap, for the memory profiler.
#[global_allocator]
static ALLOCATOR: voxear_core::memory::CountingAllocator = voxear_core::memory::CountingAllocator;

fn main() -> Result<()> {
    let data = player::data_folder()?;
    match player::open_log(&data) {
        Some(log) => voxear_core::logging::init_with_file(log),
        None => voxear_core::logging::init(),
    }
    let mut args = PlayerArgs { headless: true, net: Some(NetStart::Server(0)), ..PlayerArgs::default() };
    let mut given = std::env::args().skip(1);
    while let Some(arg) = given.next() {
        match arg.as_str() {
            "--frames" => {
                let frames = given.next().context("--frames needs a number")?;
                args.frames = Some(frames.parse().with_context(|| format!("--frames {frames} is not a number"))?);
            }
            "--port" => {
                let port = given.next().context("--port needs a number")?;
                args.port = port.parse().with_context(|| format!("--port {port} is not a port"))?;
            }
            // A host's service may pass arguments of its own.
            _ => {}
        }
    }
    player::run_player(&data, &args)
}
