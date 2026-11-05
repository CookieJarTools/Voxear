//! The Inspector for one part of a model an Obj draws whole, picked under
//! that Obj in the Hierarchy: the part's name and its Obj's, where it sits
//! in the model, which mesh it draws, and the material it draws with. What
//! is changed is kept on the Obj, as an override of the model
//! (`scene_file::PartData`), and Revert puts the part back as the model has
//! it. Add Component takes the part out into an Obj of its own inside the
//! model's, which components go on; the rest stay drawn as one.

use voxear_core::math::EulerRot;
use voxear_ecs::Entity;
use voxear_engine::{Game, scene_file::ModelSource};
use voxear_render::ModelRenderer;
use voxear_scene::Name;
use voxear_ui::{DragSource, NodeId, Style, Ui};

use crate::{
    hierarchy::part_names,
    inspector::{number, prop, vector_row},
};

/// What the part's Inspector asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum PartAction {
    /// Where the part sits in the model: its position, rotation in degrees
    /// and scale, each `None` where it is as the model puts it.
    Place([Option<[f32; 3]>; 3]),
    /// Open the object picker under `anchor` for the part's material.
    BrowseMaterial(NodeId),
    /// Something was dragged onto the part's material.
    DropMaterial(DragSource),
    /// Show where the material asset at this path is in the Project panel.
    Ping(String),
    /// Put the part back as the model has it.
    Revert,
    /// Take the part out into an Obj of its own, to add components to.
    Split,
}

pub struct PartView {
    owner: Entity,
    node: u32,
    /// Position, rotation and scale, each three fields.
    fields: [[NodeId; 3]; 3],
    /// Where the model puts the part, rotation in degrees, and where it
    /// sits now, as the fields were last shown.
    placed: [[f32; 3]; 3],
    shown: [[f32; 3]; 3],
    material: NodeId,
    arrow: NodeId,
    /// The material asset the part draws with in place of the model's.
    material_path: Option<String>,
    revert: NodeId,
    add: NodeId,
}

impl PartView {
    /// Shows node `node` of the model `owner` draws in `root`, which is
    /// emptied first. None when the Obj draws no model or no such part.
    pub fn build(ui: &mut Ui, root: NodeId, game: &Game, owner: Entity, node: u32) -> Option<Self> {
        for child in ui.children(root) {
            ui.remove(child);
        }
        let world = &game.engine.world;
        let (drawn, source) = (world.get::<ModelRenderer>(owner)?, world.get::<ModelSource>(owner)?);
        let guid = game.content.parse(&source.0.model).ok()?.guid()?;
        let nodes = game.content.loaded_model(guid)?;
        let at = nodes.get(node as usize)?;
        let names = part_names(&game.content, drawn);
        let name = names.get(node as usize).filter(|name| !name.is_empty()).cloned();
        let name = name.unwrap_or_else(|| format!("Part {}", node + 1));
        let owner_name = world.get::<Name>(owner).map(|name| name.0.clone()).unwrap_or_default();

        let header = ui.panel(root, Style::column());
        ui.add_class(header, "inspector-header");
        let top = ui.panel(header, Style::row());
        ui.add_class(top, "header-row");
        let tile = ui.panel(top, Style::row());
        ui.add_class(tile, "obj-tile");
        ui.icon(tile, "box");
        let title = ui.add_label(top, &name);
        ui.add_class(title, "asset-name");
        let of = ui.add_label(header, &format!("Part of {owner_name}"));
        ui.add_class(of, "part-of");

        let rows = section(ui, root, "Transform", "transform");
        let (y, x, z) = at.rotation.to_euler(EulerRot::YXZ);
        let placed = [at.position.to_array(), [x, y, z].map(f32::to_degrees), at.scale.to_array()];
        let changed = source.0.part(node);
        let shown = match changed {
            Some(changed) => [
                changed.position.unwrap_or(placed[0]),
                changed.rotation.unwrap_or(placed[1]),
                changed.scale.unwrap_or(placed[2]),
            ],
            None => placed,
        };
        let fields = ["Position", "Rotation", "Scale"].map(|label| vector_row(ui, rows, label).1);
        for (fields, values) in fields.iter().zip(shown) {
            for (&field, value) in fields.iter().zip(values) {
                ui.set_text(field, &number(value));
            }
        }

        let rows = section(ui, root, "Part", "box");
        let mesh = prop(ui, rows, "Mesh");
        let value = ui.add_label(mesh, &name);
        ui.add_class(value, "value");
        let row = prop(ui, rows, "Material");
        let material = ui.panel(row, Style::row());
        ui.add_class(material, "value-picker");
        ui.accept_drops(material, true);
        let icon = ui.icon(material, "material");
        ui.add_class(icon, "picker-icon");
        let material_path = changed.and_then(|changed| changed.material.as_deref()).and_then(|reference| {
            let guid = game.content.parse(reference).ok()?.guid()?;
            game.content.db().get(guid).map(|entry| entry.path.clone())
        });
        let label = match &material_path {
            Some(path) => stem(path).to_string(),
            None => {
                let named = game.content.loaded_model_materials(guid).unwrap_or_default();
                let own = at.material_part.as_ref().and_then(|part| named.iter().find(|(p, _)| p == part));
                ui.add_class(material, "picker-embedded");
                let own = own.map(|(_, name)| name.clone()).filter(|name| !name.is_empty());
                own.unwrap_or_else(|| "Model's own".into())
            }
        };
        let text = ui.add_label(material, &label);
        ui.add_class(text, "picker-label");
        let arrow = ui.icon_button(material, "chevron-down");
        ui.add_class(arrow, "picker-arrow");

        let buttons = ui.panel(root, Style::row());
        ui.add_class(buttons, "part-buttons");
        let revert = ui.button(buttons, "Revert");
        ui.add_class(revert, "add-component");
        ui.set_tooltip(revert, "Puts the part back as the model has it", None);
        ui.set_visible(revert, changed.is_some_and(|changed| changed.changes()));
        let add = ui.button(root, "Add Component");
        ui.add_class(add, "add-component");
        let tip = "Makes the part an Obj of its own inside the model's, to add components to";
        ui.set_tooltip(add, tip, None);
        Some(Self { owner, node, fields, placed, shown, material, arrow, material_path, revert, add })
    }

    /// The part shown, by its Obj and node.
    pub fn part(&self) -> (Entity, u32) {
        (self.owner, self.node)
    }

    /// What was asked this frame: a place typed, the material's field
    /// clicked, its arrow, or something dropped on it, Revert or Add
    /// Component.
    pub fn update(&mut self, ui: &mut Ui) -> Option<PartAction> {
        if ui.clicked(self.add) {
            return Some(PartAction::Split);
        }
        if ui.clicked(self.revert) {
            return Some(PartAction::Revert);
        }
        if let Some(source) = ui.dropped_onto(self.material) {
            return Some(PartAction::DropMaterial(source));
        }
        // The arrow first, as a click on it is a click on the field too.
        if ui.clicked(self.arrow) {
            return Some(PartAction::BrowseMaterial(self.material));
        }
        if let Some(path) = self.material_path.clone().filter(|_| ui.clicked(self.material)) {
            return Some(PartAction::Ping(path));
        }
        // A place is taken once typed: on Enter, or when the fields are left.
        let read = |field: NodeId, was: f32| {
            ui.text(field).trim().parse::<f32>().ok().filter(|value| value.is_finite()).unwrap_or(was)
        };
        let now: [[f32; 3]; 3] = std::array::from_fn(|vector| {
            std::array::from_fn(|axis| read(self.fields[vector][axis], self.shown[vector][axis]))
        });
        let fields = self.fields.as_flattened();
        let typing = fields.iter().any(|&field| ui.focused() == Some(field));
        let submitted = fields.iter().any(|&field| ui.submitted(field));
        if now == self.shown || (typing && !submitted) {
            return None;
        }
        self.shown = now;
        let changed = std::array::from_fn(|vector| (now[vector] != self.placed[vector]).then_some(now[vector]));
        Some(PartAction::Place(changed))
    }
}

/// A section, headed by its icon and title, returning where its rows go.
fn section(ui: &mut Ui, root: NodeId, title: &str, icon: &str) -> NodeId {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, title, true);
    ui.add_class(foldout, "section");
    ui.set_foldout_icon(foldout, icon);
    ui.foldout_content(foldout).expect("a foldout has content")
}

/// A file's name without its folder or extension.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}
