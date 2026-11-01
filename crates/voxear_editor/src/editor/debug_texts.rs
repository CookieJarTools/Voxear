//! Debug draw in the editor. Its lines are drawn by the renderer; its texts
//! (`Debug.Text`) are labels at the points they name in the view showing
//! (see `debug_texts` in voxear_engine, which a player shares). While not
//! playing, the editor counts the shapes' time down itself, as the engine
//! does each game frame.

use voxear_core::debug_draw;
use voxear_ecs::Entity;
use voxear_ui::NodeId;

use super::Editor;
use crate::scene_tools::ViewCamera;

impl Editor {
    pub(super) fn update_debug_draw(&mut self, view: Option<(NodeId, Entity)>, delta: f32) {
        if !self.is_playing() {
            debug_draw::advance(delta);
        }
        let camera = view
            .and_then(|(viewport, camera)| ViewCamera::new(&self.game.engine.world, camera, self.ui.rect(viewport)));
        let rect = camera.as_ref().map_or([0.0; 4], |camera| camera.rect);
        let project = camera.as_ref().map(|camera| move |point| camera.project(point));
        let project = project.as_ref().map(|p| p as &dyn Fn(_) -> Option<[f32; 2]>);
        self.debug_texts.update(&mut self.ui, project, rect);
    }
}
