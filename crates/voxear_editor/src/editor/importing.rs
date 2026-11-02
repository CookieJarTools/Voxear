//! Assets coming into the project, and imported again, as Unity's asset
//! importing does:
//!
//! - A sound whose import settings were applied loads with them the next
//!   time it plays; what plays it now plays on as it was.
//! - A model whose import settings were applied in the Inspector is read
//!   again with them and put behind the meshes and materials it had, so
//!   every Obj drawing it, and every undo step, shows the change at once.
//!   Parts of it placed in the open scenes move to where the model puts them
//!   now, as after a change of size, unless moved since. A picture's
//!   settings applied import again every model in use that reads it.
//! - Files dropped on the window from outside the editor are copied into the
//!   folder under the pointer in the Project panel, or else the folder it
//!   shows, unless a VoxDocs page being written takes them.
//! - When the window gets the focus back, the editor looks at the project's
//!   files: files that came or went show in the Project panel, and a model
//!   in use whose file, `.meta`, buffers or pictures changed is imported
//!   again, as above.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

use voxear_assets::meta_path;
use voxear_render::Renderer;

use super::Editor;
use crate::{import_fields::ImportSettings, project_files, project_view::ProjectEvent};

/// Folders never looked in for changes, as the asset database skips them,
/// beside those whose names start with a dot.
const SKIPPED: &[&str] = &["bin", "obj", "target", "node_modules"];

/// What the editor knows of the project's files from outside it.
#[derive(Default)]
pub(super) struct Imports {
    /// When each file last changed, by project path, as last looked at.
    times: HashMap<String, SystemTime>,
    /// Files dropped on the window, each with the folder it was let go on.
    dropped: Vec<(PathBuf, Option<String>)>,
    /// Whether to look at the files again, as after the window got the focus.
    look: bool,
}

impl Imports {
    pub(super) fn new(root: &Path) -> Self {
        Self { times: file_times(root), ..Self::default() }
    }

    /// Looks at the files at the start of the next frame.
    pub(super) fn look_again(&mut self) {
        self.look = true;
    }
}

impl Editor {
    /// Writes an asset's import settings to its `.meta`, and imports it
    /// again: a model itself, and for a picture each model in use that
    /// reads it.
    pub(super) fn apply_import_settings(&mut self, path: &str, settings: &ImportSettings, renderer: &mut Renderer) {
        let file = self.game.project.root.join(path);
        let written = match settings {
            ImportSettings::Model(settings) => settings.write(&file),
            ImportSettings::Texture(settings) => settings.write(&file),
            ImportSettings::Sound(settings) => settings.write(&file),
        };
        if let Err(error) = written {
            self.notify(&format!("Could not save the import settings of {path}: {error:#}"));
            return;
        }
        self.wrote_meta(path);
        let meta = format!("{path}.meta");
        if let ImportSettings::Model(_) = settings {
            self.reimport_model(path, renderer);
            return;
        }
        if let ImportSettings::Sound(_) = settings {
            self.game.content.forget_sound(path);
            let name = path.rsplit('/').next().unwrap_or(path);
            return self.notify(&format!("Saved the import settings of {name}; it plays with them from now on"));
        }
        let root = self.game.project.root.clone();
        let changed = HashSet::from([&meta]);
        let models: Vec<String> =
            self.game.content.loaded_models().into_iter().filter(|model| reads_any(&root, model, &changed)).collect();
        for model in &models {
            self.reimport_model(model, renderer);
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        match models.as_slice() {
            [] => self.notify(&format!("Saved the import settings of {name}; models read it with them")),
            [_] => {}
            _ => self.notify(&format!("Imported the {} models that use {name} again", models.len())),
        }
    }

    /// Notes that the editor wrote the `.meta` of the asset at `path`, so it
    /// is not a change from outside to import again for.
    pub(super) fn wrote_meta(&mut self, path: &str) {
        let file = self.game.project.root.join(path);
        if let Ok(time) = meta_path(&file).metadata().and_then(|m| m.modified()) {
            self.imports.times.insert(format!("{path}.meta"), time);
        }
    }

    /// Imports a model again with its file and the settings in its `.meta`,
    /// and draws again the previews that show it.
    pub(super) fn reimport_model(&mut self, path: &str, renderer: &mut Renderer) {
        let name = path.rsplit('/').next().unwrap_or(path);
        match self.game.reimport_model(renderer, path) {
            Ok(moved) => {
                // The parts' places are part of the scene, to save.
                self.unsaved |= moved > 0;
                self.notify(&format!("Imported {name} again"));
            }
            Err(error) => self.notify(&format!("Could not import {name} again: {error:#}")),
        }
        // Its parts' previews and those of prefabs drawing it, as well as its own.
        self.asset_previews.clear(renderer);
        self.asset_changed(renderer, path);
    }

    /// A file dropped on the window from outside the editor: into the folder
    /// under the pointer when it is over the Project panel, or else into a
    /// VoxDocs page being written, or else into the folder the Project panel
    /// shows. Brought in at the next frame, with the others dropped with it.
    pub(super) fn file_dropped(&mut self, path: &Path) {
        let pointer = self.pointer.map(|(x, y)| [x / self.scale, y / self.scale]);
        let folder = pointer.and_then(|point| self.folder_at(point));
        if folder.is_none() && self.docs.drop_file(&mut self.ui, path) {
            return;
        }
        let over_docs = self.ui.find(self.ui.root(), "docs").zip(pointer).is_some_and(|(docs, [x, y])| {
            let [left, top, width, height] = self.ui.rect(docs);
            self.ui.is_visible(docs) && (left..left + width).contains(&x) && (top..top + height).contains(&y)
        });
        if folder.is_none() && over_docs {
            self.notify("Drop pictures on a VoxDocs page while writing it, with Edit");
            return;
        }
        self.imports.dropped.push((path.to_path_buf(), folder));
    }

    /// Brings in the files dropped since the last frame, and looks at the
    /// project's files when the window got the focus back.
    pub(super) fn update_imports(&mut self, renderer: &mut Renderer) {
        if !self.imports.dropped.is_empty() {
            self.bring_in_dropped(renderer);
        }
        if std::mem::take(&mut self.imports.look) {
            self.look_at_files(renderer);
        }
    }

    fn bring_in_dropped(&mut self, renderer: &mut Renderer) {
        let dropped = std::mem::take(&mut self.imports.dropped);
        let folder =
            dropped.iter().find_map(|(_, folder)| folder.clone()).unwrap_or_else(|| self.project.open_folder());
        let files: Vec<PathBuf> = dropped.into_iter().map(|(file, _)| file).collect();
        let shown = if folder.is_empty() { self.game.project.name().to_string() } else { folder.clone() };
        match project_files::bring_in(&self.game.project.root, &folder, &files) {
            Ok(brought) => {
                let last = brought.last().cloned();
                self.project_event(ProjectEvent::Changed { select: last, rename: false }, renderer);
                let names: Vec<&str> = brought.iter().map(|path| path.rsplit('/').next().unwrap_or(path)).collect();
                self.notify(&format!("Added {} to {shown}", names.join(", ")));
            }
            Err(error) => self.notify(&format!("Could not add the files to {shown}: {error:#}")),
        }
    }

    /// Looks for what changed in the project's files while the editor was
    /// away: files that came or went show in the Project panel, and models
    /// in use whose files changed are imported again.
    fn look_at_files(&mut self, renderer: &mut Renderer) {
        let root = self.game.project.root.clone();
        let now = file_times(&root);
        let old = std::mem::replace(&mut self.imports.times, now);
        let came_or_went =
            old.len() != self.imports.times.len() || old.keys().any(|path| !self.imports.times.contains_key(path));
        let changed: HashSet<&String> = self
            .imports
            .times
            .iter()
            .filter(|(path, time)| old.get(*path).is_some_and(|was| was != *time))
            .map(|(path, _)| path)
            .collect();
        let models: Vec<String> = match changed.is_empty() {
            true => Vec::new(),
            false => self
                .game
                .content
                .loaded_models()
                .into_iter()
                .filter(|model| reads_any(&root, model, &changed))
                .collect(),
        };
        if came_or_went {
            self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
        }
        for model in models {
            self.reimport_model(&model, renderer);
        }
    }

    /// Takes the project's files as they are now as seen, so what the
    /// editor itself changed is not taken for a change from outside.
    pub(super) fn files_seen(&mut self) {
        self.imports.times = file_times(&self.game.project.root);
    }
}

/// Whether the model at `path` reads any of `files`: itself, or a buffer or
/// picture it names, or the `.meta` of any of them.
fn reads_any(root: &Path, path: &str, files: &HashSet<&String>) -> bool {
    let has = |file: &str| files.iter().any(|changed| changed.as_str() == file);
    if has(path) || has(&format!("{path}.meta")) {
        return true;
    }
    let Ok(bytes) = std::fs::read(root.join(path)) else { return false };
    let companions = voxear_render::companion_files(path, &bytes).unwrap_or_default();
    companions.iter().any(|file| has(file) || has(&format!("{file}.meta")))
}

/// When each of the project's files last changed, by project path, and its
/// folders, ending in `/`. The project's own `Docs` and the folders the
/// asset database skips are left out.
fn file_times(root: &Path) -> HashMap<String, SystemTime> {
    fn walk(folder: &Path, at: &str, times: &mut HashMap<String, SystemTime>) {
        let Ok(entries) = std::fs::read_dir(folder) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || SKIPPED.contains(&name.as_str()) || (at.is_empty() && name == "Docs") {
                continue;
            }
            let path = if at.is_empty() { name.clone() } else { format!("{at}/{name}") };
            let Ok(metadata) = entry.metadata() else { continue };
            if metadata.is_dir() {
                times.insert(format!("{path}/"), SystemTime::UNIX_EPOCH);
                walk(&entry.path(), &path, times);
            } else if let Ok(time) = metadata.modified() {
                times.insert(path, time);
            }
        }
    }
    let mut times = HashMap::new();
    walk(root, "", &mut times);
    times
}
