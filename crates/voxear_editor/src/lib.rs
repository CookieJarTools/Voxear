//! The Voxear editor: the engine with docked panels around it, built with
//! Voxear UI, in the look of the editor concept (docs/ARCHITECTURE.md, "The design").
//!
//! This first slice opens a project's start level in **edit mode** (no
//! script runs), with:
//! - a top bar: the mark, workspaces, the command bar (Ctrl+K), play
//!   controls, the account and Build; and a rail of tool icons
//! - the **Hierarchy** of Objs, filterable, and the **Project**: folders on
//!   the left and the open folder's assets and scripts on the right, as in
//!   Unity's two-column Project window
//! - the **Scene** view, rendered into its panel through the editor's own
//!   camera, flown with the right mouse button, with a tool strip, a stats
//!   chip and an axis gizmo floating over it
//! - the **Inspector**, editing the selected Obj's name and Transform and
//!   showing its components and behaviours
//! - the **Console**, showing everything logged
//! - a status bar: the level, frame rate, memory and git branch
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
