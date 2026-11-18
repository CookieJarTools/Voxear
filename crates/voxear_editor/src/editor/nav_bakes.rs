//! Baking the open level's nav meshes, from the palette's Bake Navigation, a
//! level's Inspector or `navigation.bake`: the level's still colliders and
//! terrain laid into a nav mesh (see `voxear_nav::mesh`) for the Default
//! agent type and each the project named, on a thread of its own, written
//! beside the level as its `.nav` file once done (see
//! `voxear_engine::navigation`), and put on the level at once, so agents
//! walk it from the next step.

use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

use voxear_engine::navigation;
use voxear_nav::mesh::{NavMesh, bake_ground, meshes_to_bytes};

use super::Editor;

/// A bake under way: the level's path, and what it makes.
pub(super) struct NavBake {
    level: String,
    /// Each agent type's mesh, by its slot; none where the Default type's
    /// had nothing to stand on.
    done: Receiver<Option<Vec<(u8, NavMesh)>>>,
}

impl Editor {
    /// Starts baking the open level's nav mesh.
    pub(super) fn bake_nav(&mut self) {
        if self.nav_bake.is_some() {
            return self.notify("The navigation is baking already");
        }
        if self.is_playing() {
            return self.notify("Stop playing to bake the level's navigation");
        }
        let Some(level) = self.game.levels.current().map(str::to_string) else {
            return self.notify("Open a level to bake its navigation");
        };
        let triangles = navigation::bake_triangles(&self.game.engine.world);
        let grounds = navigation::bake_grounds(&self.game.engine.world);
        // The Default agent type, and each of the project's own it has named.
        let types = self.game.project.file.navigation.agent_types();
        let kinds: Vec<(u8, voxear_nav::mesh::BakeSettings)> = types
            .iter()
            .enumerate()
            .filter(|(slot, kind)| *slot == 0 || kind.named)
            .map(|(slot, kind)| (slot as u8, kind.bake_settings()))
            .collect();
        let (given, done) = mpsc::channel();
        let started = thread::Builder::new().name("voxear-navigation".into()).spawn(move || {
            let mut meshes = Vec::new();
            for (slot, settings) in kinds {
                match bake_ground(&triangles, &grounds, settings) {
                    Some(mesh) => meshes.push((slot, mesh)),
                    None if slot == 0 => return drop(given.send(None)),
                    None => {}
                }
            }
            let _ = given.send(Some(meshes));
        });
        match started {
            Ok(_) => self.nav_bake = Some(NavBake { level, done }),
            Err(error) => self.notify(&format!("Could not start baking: {error}")),
        }
    }

    /// Bakes the navigation of the level at `path`, if it is the open one.
    pub(super) fn bake_level_nav(&mut self, path: &str) {
        if self.game.levels.current() == Some(path) {
            return self.bake_nav();
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        self.notify(&format!("Open {} to bake its navigation", name.trim_end_matches(".level")));
    }

    /// What the status bar says of a bake under way.
    pub(super) fn nav_bake_status(&self) -> Option<String> {
        self.nav_bake.as_ref().map(|_| "Baking navigation".to_string())
    }

    /// Whether a nav mesh bake is under way.
    pub(crate) fn is_baking_nav(&self) -> bool {
        self.nav_bake.is_some()
    }

    /// Writes the nav mesh beside its level once the bake is done.
    pub(super) fn finish_nav_bake(&mut self) {
        let Some(bake) = &self.nav_bake else { return };
        let Ok(baked) = bake.done.try_recv() else { return };
        let level = self.nav_bake.take().map(|bake| bake.level).unwrap_or_default();
        let name = level.rsplit('/').next().unwrap_or(&level).trim_end_matches(".level").to_string();
        let Some(meshes) = baked else {
            return self.notify(&format!("{name} has no still colliders or terrain to bake navigation from"));
        };
        let path = navigation::nav_file(&level);
        if let Err(error) = std::fs::write(self.game.project.root.join(&path), meshes_to_bytes(&meshes)) {
            tracing::error!("could not write {path}: {error}");
            return self.notify(&format!("Could not write {path}: {error}"));
        }
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look through the project again: {error:#}");
        }
        let polys = meshes.first().map_or(0, |(_, mesh)| mesh.counts().0);
        let kinds = meshes.len();
        navigation::put_meshes(&mut self.game.engine.world, meshes);
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&path));
        self.refresh_other_projects();
        let shown = self.inspector.shown_asset().filter(|item| item.path == level).cloned();
        if let Some(item) = shown {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        tracing::info!("baked {name}'s navigation for {kinds} agent types, {polys} polygons, into {path}");
        let each = if kinds > 1 { format!(" for {kinds} agent types") } else { String::new() };
        self.notify(&format!("Baked {name}'s navigation{each}: {polys} polygons"));
    }
}
