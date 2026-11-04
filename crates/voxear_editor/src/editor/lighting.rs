//! The Lighting page's changes (see `lighting_panel`): the active scene
//! names other lighting settings, a setting of the lighting it names is
//! written into that asset's file, or a copy is made to change. Each lights
//! the scene again at once.

use voxear_render::Renderer;

use super::Editor;
use crate::{
    lighting_panel::{self, LightingAction},
    project_files::unique,
    project_view::ProjectEvent,
};

/// Where new lighting settings go, in the project.
const LIGHTING_FOLDER: &str = "Lighting";

impl Editor {
    /// Applies what was changed on the Lighting page, while it is open.
    pub(super) fn update_lighting(&mut self, renderer: &mut Renderer) {
        if self.drawer != Some(super::drawer::Drawer::Lighting) {
            return;
        }
        if let Some(action) = self.lighting_panel.update(&mut self.ui) {
            self.lighting_action(action, renderer);
        }
    }

    /// The Lighting page's row for a setting, by its title.
    pub fn lighting_row(&self, title: &str) -> Option<voxear_ui::NodeId> {
        self.lighting_panel.row_of(title)
    }

    /// The lighting settings the Lighting page shows, as the scene names them.
    pub fn lighting_shown(&self) -> Option<&str> {
        self.lighting_panel.shown()
    }

    /// Shows the active scene's lighting on the page as it now is.
    pub(super) fn refresh_lighting_page(&mut self) {
        self.lighting_panel.refresh(&mut self.ui, &mut self.game);
    }

    pub(super) fn lighting_action(&mut self, action: LightingAction, renderer: &mut Renderer) {
        let done = match action {
            LightingAction::SetScene(reference) => self.set_scene_lighting(reference.as_deref()),
            LightingAction::Set { file, path, value } => {
                lighting_panel::write_setting(&file, path, value).and_then(|()| {
                    self.files_seen();
                    self.game.content.forget_kind(voxear_assets::AssetKind::Lighting);
                    let game = &mut self.game;
                    game.levels.refresh_lighting(&mut game.engine, &mut game.content)
                })
            }
            LightingAction::Copy => self.copy_lighting(renderer),
        };
        if let Err(error) = done {
            tracing::error!("{error:#}");
        }
        self.refresh_lighting_page();
    }

    /// Names other lighting settings in the active scene, which it keeps
    /// when it is saved.
    fn set_scene_lighting(&mut self, reference: Option<&str>) -> anyhow::Result<()> {
        let game = &mut self.game;
        game.levels.set_scene_lighting(&mut game.engine, &mut game.content, reference)?;
        if !self.is_playing() {
            self.unsaved = true;
        }
        Ok(())
    }

    /// Makes new lighting settings in the Lighting folder, a copy of those
    /// the scene names, and names them in the scene in their place.
    fn copy_lighting(&mut self, renderer: &mut Renderer) -> anyhow::Result<()> {
        let current = self.game.levels.scene_lighting().map(str::to_string);
        let text = lighting_panel::copy_text(&self.game, current.as_deref())?;
        let root = self.game.project.root.clone();
        let folder = root.join(LIGHTING_FOLDER);
        std::fs::create_dir_all(&folder)?;
        let name = unique(&folder, "New Lighting", ".lighting");
        std::fs::write(folder.join(&name), text)?;
        let path = format!("{LIGHTING_FOLDER}/{name}");
        self.project_event(ProjectEvent::Changed { select: Some(path.clone()), rename: false }, renderer);
        self.set_scene_lighting(Some(&path))
    }
}
