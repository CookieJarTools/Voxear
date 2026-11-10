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
//! - Import New Asset, from the Project panel's menu, asks for files with
//!   the desktop's file chooser (zenity, or KDE's kdialog), on a thread of
//!   its own so the editor keeps drawing, and copies them into the folder
//!   the menu was opened on, as a drop would.
//! - When the window gets the focus back, the editor looks at the project's
//!   files: files that came or went show in the Project panel, and a model
//!   in use whose file, `.meta`, buffers or pictures changed is imported
//!   again, as above.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    time::SystemTime,
};

use voxear_assets::Mounts;
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
    /// The project's list of assets as last looked at, by where it is kept:
    /// another once files were made from the Inspector, as a new block is.
    scanned: usize,
    /// The file chooser open for Import New Asset, and the folder its files
    /// go into.
    choosing: Option<(Receiver<std::io::Result<Vec<PathBuf>>>, String)>,
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
        // A package's from the package cache are written into the project's settings for it.
        let written = self.game.content.db().mounts().writable_meta(path).and_then(|meta| match settings {
            ImportSettings::Model(settings) => settings.write_meta(&meta),
            ImportSettings::Texture(settings) => settings.write_meta(&meta),
            ImportSettings::Sound(settings) => settings.write_meta(&meta),
        });
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
        let mounts = self.game.content.db().mounts().clone();
        let changed = HashSet::from([&meta]);
        let models: Vec<String> =
            self.game.content.loaded_models().into_iter().filter(|model| reads_any(&mounts, model, &changed)).collect();
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
        let meta = self.game.content.db().mounts().meta_file(path);
        if let Ok(time) = meta.metadata().and_then(|m| m.modified()) {
            self.imports.times.insert(format!("{path}.meta"), time);
        }
    }

    /// Imports a model again with its file and the settings in its `.meta`,
    /// and draws again the previews that show it.
    pub(super) fn reimport_model(&mut self, path: &str, renderer: &mut Renderer) {
        let name = path.rsplit('/').next().unwrap_or(path);
        let aside = self.play.edit_world();
        match self.game.reimport_model(renderer, path, aside) {
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

    /// Opens the desktop's file chooser for files to copy into `folder`.
    pub(super) fn choose_imports(&mut self, folder: String) {
        if self.imports.choosing.is_some() {
            return self.notify("A file chooser is open already");
        }
        let (send, chosen) = mpsc::channel();
        let started = std::thread::Builder::new().name("file chooser".into()).spawn(move || {
            let _ = send.send(choose_files("Import New Asset"));
        });
        match started {
            Ok(_) => self.imports.choosing = Some((chosen, folder)),
            Err(error) => self.notify(&format!("Could not open a file chooser: {error}")),
        }
    }

    /// Brings in the files dropped since the last frame, and looks at the
    /// project's files when the window got the focus back.
    pub(super) fn update_imports(&mut self, renderer: &mut Renderer) {
        if let Some((chosen, folder)) = &self.imports.choosing {
            match chosen.try_recv() {
                Ok(Ok(files)) => {
                    let folder = Some(folder.clone());
                    self.imports.dropped.extend(files.into_iter().map(|file| (file, folder.clone())));
                    self.imports.choosing = None;
                }
                Ok(Err(error)) => {
                    self.imports.choosing = None;
                    self.notify(&format!("Could not open a file chooser: {error}"));
                }
                Err(TryRecvError::Disconnected) => self.imports.choosing = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if !self.imports.dropped.is_empty() {
            self.bring_in_dropped(renderer);
        }
        // Files the Inspector made, as a Block World's new blocks, show in the Project panel at once.
        let scanned = std::ptr::from_ref(self.game.content.db()) as usize;
        if std::mem::replace(&mut self.imports.scanned, scanned) != scanned {
            self.imports.look = true;
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
        let brought = project_files::place(self.game.content.db().mounts(), &folder).and_then(|at| {
            let brought = project_files::bring_in(&at.root, &at.path, &files)?;
            Ok(brought.iter().map(|path| at.back(path)).collect::<Vec<_>>())
        });
        match brought {
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
                .filter(|model| reads_any(self.game.content.db().mounts(), model, &changed))
                .collect(),
        };
        // A string table or input actions edited outside the editor are read again; ones that came or went
        // are, with every change.
        let tables = changed.iter().any(|path| path.ends_with(".strings") || path.ends_with(".assettable"));
        let actions = changed.iter().any(|path| path.ends_with(".inputactions"));
        let lighting = changed.iter().any(|path| path.ends_with(".lighting"));
        let documents = changed.iter().any(|path| path.ends_with(".vui") || path.ends_with(".vss"));
        let prompt_icons = changed.iter().any(|path| path.ends_with(".prompticons"));
        let world_gens: Vec<String> =
            changed.iter().filter(|path| path.ends_with(".worldgen")).map(|path| path.to_string()).collect();
        let blocks = changed.iter().any(|path| path.ends_with(".block"));
        // A material may pick a built-in shader no other did, which is compiled in then.
        let shaders = changed.iter().any(|path| path.ends_with(".shader") || path.ends_with(".material"));
        if came_or_went {
            self.project_event(ProjectEvent::Changed { select: None, rename: false }, renderer);
        } else {
            if tables {
                self.game.content.forget_kind(voxear_assets::AssetKind::Strings);
                self.game.content.forget_kind(voxear_assets::AssetKind::AssetTable);
                self.game.load_strings();
            }
            if actions {
                self.game.content.forget_kind(voxear_assets::AssetKind::InputActions);
                self.game.load_input_actions();
            }
            // UI documents on the screen and on UI Panels show as edited outside the editor, keeping their handlers.
            if documents {
                self.game.ui.reload_documents(&mut self.game.content);
            }
            // Prompts show a game's own pictures for its controls as the file now has them.
            if prompt_icons {
                self.game.prompt_icons.forget();
            }
            // Terrains generated from a world generator edited outside the editor are made again from it.
            for path in &world_gens {
                self.generator_file_changed(path);
                if let Some(guid) = self.game.content.db().guid_of(path)
                    && self.game.content.forget_world_gen(guid)
                {
                    crate::terrain_props::regenerate(&mut self.game, guid);
                    crate::block_props::regenerate(&mut self.game, guid);
                }
            }
            // Blocks edited outside the editor are read again, and the worlds of blocks listing them drawn as they say.
            if blocks {
                self.game.content.forget_blocks();
            }
            // A shader saved from a code editor compiles again, and its materials draw with it; nothing
            // compiles again when the shaders the materials pick are as they were.
            if shaders {
                self.game.apply_custom_shaders(renderer);
            }
            // Lighting settings edited outside the editor light the open scene as they now are.
            if lighting {
                self.game.content.forget_kind(voxear_assets::AssetKind::Lighting);
                let game = &mut self.game;
                if let Err(error) = game.levels.refresh_lighting(&mut game.engine, &mut game.content) {
                    tracing::error!("{error:#}");
                }
            }
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
fn reads_any(mounts: &Mounts, path: &str, files: &HashSet<&String>) -> bool {
    let has = |file: &str| files.iter().any(|changed| changed.as_str() == file);
    if has(path) || has(&format!("{path}.meta")) {
        return true;
    }
    let Ok(bytes) = std::fs::read(mounts.file(path)) else { return false };
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

/// Files picked in the desktop's file chooser, or none when it is closed
/// without picking: zenity's, as GNOME and most desktops have, or else
/// KDE's kdialog.
fn choose_files(title: &str) -> std::io::Result<Vec<PathBuf>> {
    let choosers: [(&str, &[&str]); 2] = [
        ("zenity", &["--file-selection", "--multiple", "--separator=\n", "--title", title]),
        ("kdialog", &["--getopenfilename", ".", "--multiple", "--separate-output", "--title", title]),
    ];
    for (program, args) in choosers {
        match std::process::Command::new(program).args(args).output() {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                return Ok(text.lines().filter(|line| !line.is_empty()).map(PathBuf::from).collect());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    let missing = "no file chooser was found; install zenity, or drag the files onto the Project panel";
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, missing))
}
