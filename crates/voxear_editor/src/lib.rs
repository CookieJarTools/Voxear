//! The Voxear editor: the engine with docked panels around it, built with
//! Voxear UI, in the look of the editor concept (docs/ARCHITECTURE.md, "The design").
//!
//! This first slice opens a project's start level in **edit mode** (no
//! script runs), with:
//! - a top bar: the mark, the Layout menu, the command bar with its palette
//!   (Ctrl+K) of commands, settings, Objs, levels, assets and docs, which
//!   scripts' `[Command]` methods and [`PaletteCommand`]s add to, play
//!   controls, the account and Build; and a rail of tool icons
//! - the **Hierarchy** of Objs, filterable, and the **Project**: folders on
//!   the left and the open folder's assets and scripts on the right, as in
//!   Unity's two-column Project window
//! - the **Scene** view, rendered into its panel through the editor's own
//!   camera, flown with the right mouse button, with a tool strip, a stats
//!   chip and an axis gizmo floating over it, and the **Game** view, through
//!   the level's camera
//! - **play mode**: Play runs a copy of the edited level with its scripts,
//!   Pause and Step hold it and move it on, and Stop brings the edited level
//!   back as it was; scripts that change reload, into the running game while
//!   playing, with what their behaviours hold carried over
//! - the Scene view's **tools**: click an Obj to select it, and drag the
//!   move, rotate or scale handles around it (Q, W, E and R pick the tool;
//!   Ctrl snaps)
//! - **editing Objs** in the Hierarchy: its + button and context menus make,
//!   duplicate and delete them, and dragging one onto another reparents it;
//!   a prefab or model dragged from the Project onto it or the Scene view
//!   places it there, and a scene opens alongside the level, additively
//! - the **Inspector**, editing the selected Obj's name, Transform, whether
//!   it is persistent, and components, its scripts' fields included, with Add
//!   and Remove Component; while playing, a row's menu can keep its value
//!   after play; for a level selected in the Project, its scenes, added,
//!   taken out and marked active or not required
//! - the **Console**, after Sifter: categories nobody declared, a search with
//!   tags and regular expressions, and each line's stack coloured and folded,
//!   its frames opening their files
//! - the **VoxDocs**: the project's own documentation, Markdown pages from its
//!   `Docs` folder (see `voxear_docs`)
//! - the **Levels** panel, as LayerCake's window: every level and loose
//!   scene, to open, play, or while playing, change to
//! - the **Generator** panel: a World Generator's graph of nodes on a canvas,
//!   with a picture on every node of what it makes and the ground the whole
//!   graph makes beside it, in 3D (see `generator_panel`)
//! - the **Package Manager**, as Unity's: the project's packages with their
//!   updates, added from Git, a folder or the registry, and asked about
//!   first when one brings code (see `package_manager`)
//! - a status bar: what the editor is doing, frame rate, memory and git
//!   branch
//! - saving with Ctrl+S, and undo and redo of every edit with Ctrl+Z and Ctrl+Y
//! - **Settings**, kept per project with what the palette ran recently

mod asset_inspector;
mod attributes;
pub mod audio_panel;
mod build_window;
mod collection;
mod columns;
mod curve_fields;
mod console;
mod data_assets;
mod editor;
pub mod entities_panel;
mod field_drawers;
mod field_handles;
mod path_handles;
mod fields;
mod frame_debugger_panel;
pub mod generator_panel;
pub mod voxshade_panel;
mod generator_previews;
pub mod box_part_inspector;
pub mod model_panel;
pub mod model_sheet;
pub mod model_timeline;
pub mod model_view;
mod gizmos;
pub mod graph_canvas;
mod gun_handling_inspector;
mod hierarchy;
mod history;
mod import_fields;
mod input_edit;
mod input_editor;
mod input_panel;
mod inspector;
mod layers_view;
mod level_files;
mod levels_panel;
pub mod link;
mod lighting_panel;
pub mod localization_panel;
mod localization_tables;
mod live;
mod lod_inspector;
pub mod memory_panel;
mod object_picker;
mod objs;
pub mod package_list;
pub mod package_manager;
mod palette;
mod part_inspector;
mod profiler;
mod project_files;
mod project_view;
mod props;
mod block_inspector;
mod biome_inspector;
pub mod block_database;
mod block_panel;
mod block_props;
mod block_tools;
mod plants_panel;
mod heat_props;
mod storm_props;
mod plants_props;
mod plants_tools;
mod terrain_panel;
mod terrain_props;
mod terrain_tools;
pub mod scene_tools;
mod script_handles;
mod settings;
mod project_settings;
mod settings_panel;
mod shader_fields;
mod motion_set_inspector;
mod prompt_icons_inspector;
mod world_gen_inspector;
mod starter_biomes;
mod starter_blocks;
mod starter_trees;
mod strings_inspector;
#[cfg(target_os = "linux")]
mod wayland_drops;
mod window;

use std::path::Path;

use anyhow::{Context, Result};
use voxear_core::logging::LogCapture;
use voxear_engine::Project;
use voxear_render::{Renderer, RendererSettings};

pub use editor::{Drawer, Editor, SourceOpener};
pub use hierarchy::EditorOnly;
pub use palette::{PaletteCommand, PaletteKey};
pub use profiler::{Profiler, Sample};
pub use project_view::{ItemKind, ProjectItem};
pub use settings::{PlayAs, SETTINGS, SettingInfo, SettingKind, SettingValue, Settings, TestNetwork};
pub use window::run_editor;

/// Runs the editor without a window, answering tools over its link, as
/// `vox editor serve` does, until one asks it to close with `editor.quit`.
/// It draws offscreen at `size`, so captures work as in a window.
pub fn serve(project: Project, settings: &RendererSettings, log: LogCapture, (width, height): (u32, u32)) -> Result<()> {
    let mut renderer = Renderer::new_offscreen(width, height, settings)?;
    let mut editor = Editor::open_without_memory(project, &mut renderer, log)?;
    editor.serve_link(true)?;
    let frame = std::time::Duration::from_secs_f32(1.0 / 30.0);
    let mut last = std::time::Instant::now();
    while !editor.wants_to_quit() {
        let started = std::time::Instant::now();
        let delta = (started - last).as_secs_f32().min(0.1);
        last = started;
        editor.update(&mut renderer, (width, height), 1.0, delta)?;
        if let Some(rest) = frame.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    Ok(())
}

/// Opens the editor without a window for some calls, as `vox editor call
/// --headless` does where no editor is open, makes them in order, and
/// closes. Each call's answer comes back; nothing is saved unless a call
/// saves.
pub fn call_headless(
    project: Project,
    settings: &RendererSettings,
    log: LogCapture,
    (width, height): (u32, u32),
    calls: &[(String, serde_json::Value)],
) -> Result<Vec<voxear_engine::local_link::Answer>> {
    let mut renderer = Renderer::new_offscreen(width, height, settings)?;
    let mut editor = Editor::open_without_memory(project, &mut renderer, log)?;
    // Laid out first, so the views have their places.
    for _ in 0..2 {
        editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
    }
    let mut answers = Vec::new();
    for (call, args) in calls {
        let answer = editor.call(call, args.clone());
        let answered = loop {
            editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
            match answer.try_recv() {
                Ok(answered) => break answered,
                Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => anyhow::bail!("the editor dropped the call {call}"),
            }
        };
        answers.push(answered);
        if editor.wants_to_quit() {
            break;
        }
    }
    Ok(answers)
}

/// Asks a headless editor serving the project, as `vox mcp` starts one, to
/// save and close, so the editor opening now has the project and its link.
/// A windowed editor open on it already keeps its link; this one goes on
/// without one.
pub fn take_over_from_headless(root: &Path) -> Result<()> {
    let folder = link::link_folder(root);
    let Ok(mut other) = voxear_engine::local_link::LinkClient::connect(&folder, link::LINK_FILE) else { return Ok(()) };
    if !other.about.headless {
        return Ok(());
    }
    tracing::info!("taking the project over from the headless editor (process {})", other.about.pid);
    other.call("editor.quit", serde_json::json!({ "save": true }))?.map_err(anyhow::Error::from)?;
    let file = folder.join(link::LINK_FILE);
    let asked = std::time::Instant::now();
    while file.exists() && asked.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(())
}

/// Renders the editor offscreen after `frames` frames, with the first Obj
/// that has a mesh selected, and writes it to `out` as a PNG. For checking the
/// editor without a window, and for documentation.
///
/// With `open`, an asset's path in the project, the asset is opened first,
/// as a double click on it in the Project does. A World Generator's graph
/// shows once its previews are made; `Generators/Hills.worldgen#Noise`
/// picks its first node of that name too, which the Inspector then shows.
/// A box model opens in the Model window, and `Models/Pig.boxmodel#Head`
/// picks that bone's first box too.
pub fn screenshot(
    project: Project,
    settings: &RendererSettings,
    log: LogCapture,
    out: &Path,
    (width, height): (u32, u32),
    frames: u32,
    open: Option<&str>,
) -> Result<()> {
    let mut renderer = Renderer::new_offscreen(width, height, settings)?;
    let mut editor = Editor::open_without_memory(project, &mut renderer, log)?;
    let selected = editor.first_mesh();
    for frame in 0..frames.max(2) {
        if frame == 1 {
            editor.select(selected);
            if let Some(open) = open {
                let (path, node) = open.split_once('#').map_or((open, None), |(path, node)| (path, Some(node)));
                anyhow::ensure!(editor.open_path(path, &mut renderer), "{path} is not in the project");
                // Laid out and drawn before a node of it is picked.
                editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
                editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
                match node {
                    // A box model's bone, by its name: its first box where it has one, or the bone.
                    Some(bone) if voxear_render::is_box_model(path) => {
                        anyhow::ensure!(editor.pick_model_bone(bone), "{path} has no bone {bone}");
                    }
                    Some(node) => {
                        anyhow::ensure!(editor.pick_generator_node(node), "the graph of {path} has no {node} node");
                    }
                    None => {}
                }
            }
        }
        editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
    }
    // A graph's previews are made on a thread of their own: the picture waits for them, and for them to be drawn.
    if editor.generator().path().is_some() {
        let waited = std::time::Instant::now();
        while !editor.generator().previews_settled() && waited.elapsed() < std::time::Duration::from_secs(60) {
            std::thread::sleep(std::time::Duration::from_millis(10));
            editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
        }
        for _ in 0..4 {
            editor.update(&mut renderer, (width, height), 1.0, 1.0 / 60.0)?;
        }
    }
    let (width, height, pixels) = renderer.read_pixels()?;
    let file = std::fs::File::create(out).with_context(|| format!("creating {}", out.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
