//! The Voxear editor: the engine with docked panels around it, built with
//! Voxear UI, in the look of the editor concept (docs/ARCHITECTURE.md).
//!
//! This first slice opens a project's start level in **edit mode** (no
//! script runs), with:
//! - a top bar: workspaces, the command bar (Ctrl+K), play controls, Build
//! - the **Hierarchy** of Objs, filterable, and the **Project**'s assets
//! - the **Scene** view, rendered into its panel through the editor's own
//!   camera, flown with the right mouse button
//! - the **Inspector**, editing the selected Obj's name and Transform and
//!   showing its components and behaviours
//! - the **Console**, showing everything logged
//! - a status bar: the level, frame rate, GPU time and git branch
//!
//! Play mode, saving, gizmos and undo come next.

mod console;
mod editor;
mod hierarchy;
mod inspector;
mod project_view;
mod window;

use std::path::Path;

use anyhow::{Context, Result};
use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};

pub use editor::Editor;
pub use hierarchy::EditorOnly;
pub use window::run_editor;

/// Renders the editor offscreen after `frames` frames, with the first Obj
/// that has a mesh selected, and writes it to `out` as a PNG. For checking the
/// editor without a window, and for documentation.
pub fn screenshot(
    project: Project,
    settings: &RendererSettings,
    log: LogCapture,
    out: &Path,
    (width, height): (u32, u32),
    frames: u32,
) -> Result<()> {
    let mut renderer = Renderer::new_offscreen(width, height, settings)?;
    let mut editor = Editor::open(project, &mut renderer, log)?;
    let selected = editor.first_mesh();
    for frame in 0..frames.max(2) {
        if frame == 1 {
            editor.select(selected);
        }
        editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
    }
    let (width, height, pixels) = renderer.read_pixels()?;
    let file = std::fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
