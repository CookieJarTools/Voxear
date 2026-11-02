//! Materials dragged from the Project onto an Obj, as in Unity. Over a
//! Scene view, the Obj under the pointer shows the material while it is
//! there, and gets its own back when the pointer moves off or the drag
//! ends elsewhere; let go of, the Obj keeps it. Let go of on an Obj's row
//! in the Hierarchy, that Obj takes it. Either is one step to undo, made as
//! the Inspector's object picker makes it. A model's material, from the
//! Project's opened-out model, drops the same way. Only an Obj that draws a
//! mesh takes one.

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_engine::MaterialSource;
use voxear_render::{MaterialHandle, MeshRenderer, Renderer};
use voxear_ui::DragSource;

use super::Editor;
use crate::{fields::Pick, inspector::RowAction, objs::Destination, props::Prop, scene_tools};

/// A material shown on an Obj while it is dragged over it, and the Obj's
/// own, to put back.
pub(super) struct MaterialPreview {
    entity: Entity,
    own: MaterialHandle,
}

impl Editor {
    /// The material being dragged from the Project, as a Mesh Renderer's
    /// Material field takes it, when one is.
    pub(super) fn dragged_material(&self, source: &DragSource) -> Option<String> {
        let path = self.dragged_asset(source)?;
        if path.contains("#material") {
            return Some(path);
        }
        let db = self.game.content.db();
        let kind = db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind)?;
        (kind == AssetKind::Material).then_some(path)
    }

    /// Shows the dragged material on the Obj under the pointer, puts the
    /// Obj's own back when the pointer moves off, and gives it to the Obj it
    /// is let go of on.
    pub(super) fn update_material_drops(&mut self, renderer: &mut Renderer) {
        let in_view = self.all_scene_views().find_map(|view| self.ui.dropped_onto(view.viewport));
        let in_view =
            in_view.and_then(|source| Some((self.dragged_material(&source)?, self.material_target(renderer))));
        let on_row = self.dropped_on_hierarchies().and_then(|(source, to)| {
            let entity = match to? {
                Destination::Into(entity) | Destination::Before(entity) | Destination::After(entity) => entity,
                Destination::SceneEnd(_) => return None,
            };
            Some((self.dragged_material(&source)?, Some(entity)))
        });
        if let Some((material, target)) = in_view.or(on_row) {
            self.end_material_preview();
            match target.filter(|&entity| self.game.engine.world.has::<MeshRenderer>(entity)) {
                Some(entity) => {
                    let pick = RowAction::Pick(Prop::Material, Pick::Asset(Some(material)));
                    self.row_action_on(entity, pick, renderer);
                }
                None => self.notify("Drop a material on an Obj that draws a mesh"),
            }
        }

        let dragged = self.ui.dragging().and_then(|source| self.dragged_material(&source));
        let over = dragged.and_then(|material| Some((material, self.material_target(renderer)?)));
        match over {
            Some((_, entity)) if self.material_preview.as_ref().is_some_and(|shown| shown.entity == entity) => {}
            Some((material, entity)) => {
                self.end_material_preview();
                self.start_material_preview(entity, &material, renderer);
            }
            None => self.end_material_preview(),
        }
    }

    /// The Obj drawing a mesh under the pointer in the Scene view in use.
    fn material_target(&self, renderer: &Renderer) -> Option<Entity> {
        let camera = self.scene_camera()?;
        let (x, y) = self.ui.pointer_position()?;
        let [left, top, width, height] = camera.rect;
        if !(left..left + width).contains(&x) || !(top..top + height).contains(&y) {
            return None;
        }
        let hit = scene_tools::pick(&self.game.engine.world, renderer, &camera, [x, y])?;
        self.game.engine.world.has::<MeshRenderer>(hit).then_some(hit)
    }

    fn start_material_preview(&mut self, entity: Entity, material: &str, renderer: &mut Renderer) {
        let Some(own) = self.game.engine.world.get::<MeshRenderer>(entity).map(|drawn| drawn.material) else {
            return;
        };
        let Ok(reference) = self.game.content.parse(material) else { return };
        let Ok(shown) = self.game.content.material(renderer, &MaterialSource::Asset(reference)) else { return };
        if let Some(drawn) = self.game.engine.world.get_mut::<MeshRenderer>(entity) {
            drawn.material = shown;
        }
        self.material_preview = Some(MaterialPreview { entity, own });
    }

    /// Puts back the material of the Obj the dragged one was shown on.
    fn end_material_preview(&mut self) {
        let Some(shown) = self.material_preview.take() else { return };
        if let Some(drawn) = self.game.engine.world.get_mut::<MeshRenderer>(shown.entity) {
            drawn.material = shown.own;
        }
    }
}
