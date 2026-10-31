//! `voxear-player`: runs a game `vox build` built, from the `Data` folder
//! beside it. A build copies it in under the game's name.
//!
//! `--headless` runs without a window or GPU, and `--frames <n>` stops after
//! that many frames, for checking a build.

use anyhow::{Context, Result};
use voxear_engine::player::{self, PlayerArgs};

fn main() -> Result<()> {
    let data = player::data_folder()?;
    player::choose_validation(&data);
    voxear_core::logging::init();
    let mut args = PlayerArgs::default();
    let mut given = std::env::args().skip(1);
    while let Some(arg) = given.next() {
        match arg.as_str() {
            "--headless" => args.headless = true,
            "--frames" => {
                let frames = given.next().context("--frames needs a number")?;
                args.frames = Some(frames.parse().with_context(|| format!("--frames {frames} is not a number"))?);
            }
            // A store or launcher may pass arguments of its own.
            _ => {}
        }
    }
    player::run_player(&data, &args)
}
