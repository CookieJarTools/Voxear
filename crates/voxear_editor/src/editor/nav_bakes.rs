//! Baking the open level's nav mesh, from the palette's Bake Navigation, a
//! level's Inspector or `navigation.bake`: the level's still colliders laid
//! into a nav mesh (see `voxear_nav::mesh`) on a thread of its own, written
//! beside the level as its `.nav` file once done (see
//! `voxear_engine::navigation`), and put on the level at once, so agents
//! walk it from the next step.

use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread,
};

use voxear_engine::navigation::{self, LevelNav};
use voxear_nav::mesh::{BakeSettings, NavMesh, bake_ground};

use super::Editor;

/// A bake under way: the level's path, and what it makes.
pub(super) struct NavBake {
    level: String,
    done: Receiver<Option<NavMesh>>,
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
        let (given, done) = mpsc::channel();
        let started = thread::Builder::new().name("voxear-navigation".into()).spawn(move || {
            let _ = given.send(bake_ground(&triangles, &grounds, BakeSettings::default()));
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
        let Some(mesh) = baked else {
            return self.notify(&format!("{name} has no still colliders or terrain to bake navigation from"));
        };
        let path = navigation::nav_file(&level);
        if let Err(error) = std::fs::write(self.game.project.root.join(&path), mesh.to_bytes()) {
            tracing::error!("could not write {path}: {error}");
            return self.notify(&format!("Could not write {path}: {error}"));
        }
        if let Err(error) = self.game.content.rescan() {
            tracing::warn!("could not look through the project again: {error:#}");
        }
        let (polys, ..) = mesh.counts();
        self.game.engine.world.insert_resource(LevelNav(Arc::new(mesh)));
        self.project.refresh(&mut self.ui, self.game.content.db(), Some(&path));
        self.refresh_other_projects();
        let shown = self.inspector.shown_asset().filter(|item| item.path == level).cloned();
        if let Some(item) = shown {
            self.inspector.show_asset(&mut self.ui, &self.game, &item);
        }
        tracing::info!("baked {name}'s navigation, {polys} polygons, into {path}");
        self.notify(&format!("Baked {name}'s navigation: {polys} polygons"));
    }
}
