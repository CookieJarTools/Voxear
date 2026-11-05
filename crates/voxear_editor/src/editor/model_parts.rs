//! A part of a model an Obj draws whole, picked under it in the Hierarchy
//! and shown in the Inspector (see `part_inspector`): moving it, drawing it
//! with another material, putting it back as the model has it, or taking it
//! out into an Obj of its own to add components to. Each change is kept on
//! the Obj (`scene_file::PartData`), saved with the scene, and one step to
//! undo.

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_engine::scene_file::PartData;
use voxear_render::Renderer;
use voxear_ui::NodeId;

use super::Editor;
use crate::{
    fields::Pick,
    history::{Edit, ObjState},
    object_picker::PickFor,
    objs,
    part_inspector::PartAction,
};

impl Editor {
    /// Carries out what the part shown in the Inspector asks.
    pub(super) fn part_action(&mut self, action: PartAction, renderer: &mut Renderer) {
        let Some((owner, node)) = self.inspector.shown_part() else { return };
        match action {
            PartAction::Place([position, rotation, scale]) => {
                let place = |part: &mut PartData| {
                    (part.position, part.rotation, part.scale) = (position, rotation, scale);
                };
                self.change_part(owner, node, renderer, place);
            }
            PartAction::BrowseMaterial(anchor) => self.browse_part_material(owner, node, anchor),
            PartAction::DropMaterial(source) => {
                let Some(path) = self.dragged_asset(&source) else { return };
                let db = self.game.content.db();
                if db.guid_of(&path).and_then(|guid| db.get(guid)).map(|entry| entry.kind) != Some(AssetKind::Material) {
                    return self.notify("Only a material goes in a part's material");
                }
                self.pick_part_material(owner, node, &Pick::Asset(Some(path)), renderer);
            }
            PartAction::Ping(path) => self.ping_asset(&path),
            PartAction::Revert => {
                let revert = |part: &mut PartData| *part = PartData { node, apart: part.apart, ..PartData::default() };
                self.change_part(owner, node, renderer, revert);
            }
            PartAction::Split => self.split_part(owner, node, renderer),
        }
    }

    /// Changes a part as `change` says, as one step to undo, and shows it
    /// again.
    fn change_part(&mut self, owner: Entity, node: u32, renderer: &mut Renderer, change: impl FnOnce(&mut PartData)) {
        let Some(before) = ObjState::of(&self.game.engine.world, owner) else { return };
        if let Err(error) = objs::change_part(&mut self.game, renderer, owner, node, change) {
            return self.notify(&format!("Could not change the part: {error:#}"));
        }
        if let Some(after) = ObjState::of(&self.game.engine.world, owner).filter(|after| *after != before)
            && !self.is_playing()
        {
            self.history.record_step(Edit::Changed { entity: owner, before, after });
            self.unsaved = true;
        }
        let playing = self.is_playing();
        self.inspector.show_part(&mut self.ui, &self.game, owner, node, playing);
    }

    /// Opens the object picker under `anchor` for a part's material.
    fn browse_part_material(&mut self, owner: Entity, node: u32, anchor: NodeId) {
        let choices = self.asset_choices(AssetKind::Material);
        self.picker.open(&mut self.ui, (PickFor::PartMaterial { owner, node }, anchor), "Select Material", choices, "");
    }

    /// Draws a part with the material asset picked, or with the model's own
    /// again for none.
    pub(super) fn pick_part_material(&mut self, owner: Entity, node: u32, pick: &Pick, renderer: &mut Renderer) {
        let material = match pick {
            Pick::Asset(Some(path)) => match self.game.content.db().guid_of(path) {
                Some(guid) => Some(guid.to_string()),
                None => return,
            },
            _ => None,
        };
        self.change_part(owner, node, renderer, |part| part.material = material);
    }

    /// Takes a part out into an Obj of its own inside the model's, as one
    /// step to undo, and selects it, ready for components.
    fn split_part(&mut self, owner: Entity, node: u32, renderer: &mut Renderer) {
        let Some(before) = ObjState::of(&self.game.engine.world, owner) else { return };
        let made = match objs::split_part(&mut self.game, renderer, owner, node) {
            Ok(made) => made,
            Err(error) => return self.notify(&format!("Could not take the part out: {error:#}")),
        };
        if let Some(after) = ObjState::of(&self.game.engine.world, owner)
            && !self.is_playing()
        {
            let edits = vec![Edit::Changed { entity: owner, before, after }, Edit::Exists { root: made, taken: None }];
            self.history.record_step(Edit::Group(edits));
            self.unsaved = true;
        }
        self.refresh_hierarchies();
        self.select(Some(made));
    }
}
