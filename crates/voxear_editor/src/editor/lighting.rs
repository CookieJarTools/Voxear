//! The Lighting page's changes (see `lighting_panel`), and a scene's
//! Inspector's: the scene names other lighting settings, a setting of the
//! lighting it names is written into that asset's file, or a copy is made
//! to change. Each lights the scene again at once.

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

    /// A look pack's file changed: read again, and worn again as it now is
    /// where the scene is shown under one.
    pub(super) fn look_pack_changed(&mut self) {
        self.game.content.forget_kind(voxear_assets::AssetKind::LookPack);
        let game = &mut self.game;
        let worn = voxear_engine::look_packs::worn(&game.engine.world).map(str::to_string);
        let again = worn.map(|reference| {
            voxear_engine::look_packs::wear(&mut game.engine.world, &mut game.content, &reference)
        });
        if let Some(Err(error)) = again {
            tracing::error!("{error:#}");
        }
    }

    /// The Lighting page's menu of look packs to show the scene under.
    pub fn lighting_look_menu(&self) -> voxear_ui::NodeId {
        self.lighting_panel.look_menu()
    }

    /// Shows the active scene's lighting on the page as it now is, and the
    /// lighting the Inspector shows.
    pub(super) fn refresh_lighting_page(&mut self) {
        self.lighting_panel.refresh(&mut self.ui, &mut self.game);
        if let Some(view) = self.inspector.asset_view() {
            view.refresh_lighting(&mut self.ui, &self.game);
        }
    }

    pub(super) fn lighting_action(&mut self, action: LightingAction, renderer: &mut Renderer) {
        let done = match action {
            LightingAction::SetScene(reference) => self.set_scene_lighting(reference.as_deref()),
            // A look pack's setting: written into its file, and the scene shown under it wears it as it now is.
            LightingAction::Set { file, path, value } if file.extension().is_some_and(|e| e == "lookpack") => {
                lighting_panel::write_setting(&file, path, value).map(|()| {
                    self.files_seen();
                    self.look_pack_changed();
                })
            }
            LightingAction::Set { file, path, value } => {
                lighting_panel::write_setting(&file, path, value).and_then(|()| {
                    self.files_seen();
                    self.game.content.forget_kind(voxear_assets::AssetKind::Lighting);
                    let game = &mut self.game;
                    game.levels.refresh_lighting(&mut game.engine, &mut game.content)
                })
            }
            LightingAction::Copy => self.copy_lighting(renderer),
            LightingAction::Wear(reference) => {
                let game = &mut self.game;
                let reference = reference.as_deref().unwrap_or_default();
                voxear_engine::look_packs::wear(&mut game.engine.world, &mut game.content, reference)
            }
        };
        if let Err(error) = done {
            tracing::error!("{error:#}");
        }
        self.refresh_lighting_page();
    }

    /// A change to the lighting of the scene at `scene`, from its Inspector:
    /// the open scene's is changed as the Lighting page changes it, and kept
    /// when it is saved; a scene that is not open has it written into its
    /// file.
    pub(super) fn scene_lighting_action(&mut self, scene: &str, action: LightingAction, renderer: &mut Renderer) {
        let done = match action {
            LightingAction::Set { .. } | LightingAction::Wear(_) => return self.lighting_action(action, renderer),
            LightingAction::SetScene(reference) => self.set_lighting_of(scene, reference.as_deref()),
            LightingAction::Copy => self.copy_lighting_of(scene, renderer),
        };
        if let Err(error) = done {
            self.notify(&format!("Could not change the lighting of {scene}: {error:#}"));
        }
        self.refresh_lighting_page();
    }

    /// Names other lighting settings, or none, in the scene at `scene`.
    fn set_lighting_of(&mut self, scene: &str, reference: Option<&str>) -> anyhow::Result<()> {
        let game = &mut self.game;
        if game.levels.lighting_of(scene).is_some() {
            game.levels.set_lighting_of(&mut game.engine, &mut game.content, scene, reference)?;
            if !self.is_playing() {
                self.unsaved = true;
            }
            return Ok(());
        }
        let file = game.content.db().file(scene);
        lighting_panel::write_setting(&file, &["lighting"], reference.map(toml_edit::Value::from))?;
        self.files_seen();
        Ok(())
    }

    /// Makes new lighting settings in the Lighting folder, a copy of those
    /// the scene at `scene` names, or of its own sky and post-processing,
    /// and names them in the scene in their place.
    fn copy_lighting_of(&mut self, scene: &str, renderer: &mut Renderer) -> anyhow::Result<()> {
        let (current, own) = lighting_panel::scene_lighting(&self.game, scene)?;
        let text = match current {
            Some(reference) => lighting_panel::copy_text(&self.game, Some(&reference))?,
            None => lighting_panel::own_text(&own)?,
        };
        let path = self.write_new_lighting(&text, renderer)?;
        self.set_lighting_of(scene, Some(&path))
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
        let path = self.write_new_lighting(&text, renderer)?;
        self.set_scene_lighting(Some(&path))
    }

    /// Writes new lighting settings into the Lighting folder, and selects
    /// them in the Project panel. Returns their project path.
    fn write_new_lighting(&mut self, text: &str, renderer: &mut Renderer) -> anyhow::Result<String> {
        let folder = self.game.project.root.join(LIGHTING_FOLDER);
        std::fs::create_dir_all(&folder)?;
        let name = unique(&folder, "New Lighting", ".lighting");
        std::fs::write(folder.join(&name), text)?;
        let path = format!("{LIGHTING_FOLDER}/{name}");
        self.project_event(ProjectEvent::Changed { select: Some(path.clone()), rename: false }, renderer);
        Ok(path)
    }
}
