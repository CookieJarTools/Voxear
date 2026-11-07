//! `voxear-player`: runs a game `vox build` built, from the `Data` folder
//! beside it. A build copies it in under the game's name.
//!
//! `--headless` runs without a window or GPU, and `--frames <n>` stops after
//! that many frames, for checking a build. `--server` serves the game
//! without a window, `--host` hosts it with a player of its own, and
//! `--connect <address>` joins a server; `--port <n>` picks the port a
//! server or host listens on.

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
    player::choose_validation(&data);
    match player::open_log(&data) {
        Some(log) => voxear_core::logging::init_with_file(log),
        None => voxear_core::logging::init(),
    }
    let mut args = PlayerArgs::default();
    let mut given = std::env::args().skip(1);
    while let Some(arg) = given.next() {
        match arg.as_str() {
            "--headless" => args.headless = true,
            "--frames" => {
                let frames = given.next().context("--frames needs a number")?;
                args.frames = Some(frames.parse().with_context(|| format!("--frames {frames} is not a number"))?);
            }
            "--port" => {
                let port = given.next().context("--port needs a number")?;
                args.port = port.parse().with_context(|| format!("--port {port} is not a port"))?;
            }
            "--server" => args.net = Some(NetStart::Server(0)),
            "--host" => args.net = Some(NetStart::Host(0)),
            "--connect" => {
                let address = given.next().context("--connect needs an address, such as 127.0.0.1:7777")?;
                let address = address.parse().with_context(|| format!("--connect {address} is not an address"))?;
                args.net = Some(NetStart::Connect(address));
            }
            // A store or launcher may pass arguments of its own.
            _ => {}
        }
    }
    player::run_player(&data, &args)
}
