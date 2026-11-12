//! A bone or a box of the box model open in the Model window, in the
//! Inspector: a bone's name, pivot and rotation, or a box's position, size
//! and rotation and where its faces are on the sheet. What is typed or
//! dragged in a row comes back as a [`BoxPartEdit`] for the window to make
//! of it a step to undo, and rows follow the model as it changes under
//! them, as when a handle moves what they show.

use voxear_core::math::Vec3;
use voxear_render::{BoneRole, BoxModel, BoxSkin};
use voxear_ui::{NodeId, Style, Ui};

use crate::{
    asset_inspector::part,
    inspector::{prop, read_numbers, vector_row, write_numbers},
    model_view::Picked,
};

/// A change typed into the rows, to the part they show.
#[derive(Clone, Debug, PartialEq)]
pub enum BoxPartEdit {
    Name(String),
    /// What part of the body a bone moves.
    Role(BoneRole),
    Pivot(Vec3),
    BoneRotation(Vec3),
    Position(Vec3),
    Size([u32; 3]),
    BoxRotation(Vec3),
    /// Where its faces are unfolded on the sheet.
    Sheet([u32; 2]),
    Mirror(bool),
    Inflate(f32),
}

impl BoxPartEdit {
    /// Which row it came from, for changes to one row close together to be
    /// one step to undo.
    pub fn row(&self) -> u8 {
        match self {
            BoxPartEdit::Name(_) => 0,
            BoxPartEdit::Pivot(_) | BoxPartEdit::Position(_) => 1,
            BoxPartEdit::BoneRotation(_) | BoxPartEdit::BoxRotation(_) => 2,
            BoxPartEdit::Size(_) => 3,
            BoxPartEdit::Sheet(_) => 4,
            BoxPartEdit::Mirror(_) => 5,
            BoxPartEdit::Inflate(_) => 6,
            BoxPartEdit::Role(_) => 7,
        }
    }
}

/// The rows of a bone, or of a box.
enum Rows {
    Bone {
        name: NodeId,
        role: NodeId,
        pivot: [NodeId; 3],
        rotation: [NodeId; 3],
    },
    Box {
        position: [NodeId; 3],
        size: [NodeId; 3],
        rotation: [NodeId; 3],
        sheet: Option<[NodeId; 2]>,
        mirror: Option<NodeId>,
        inflate: NodeId,
    },
}

pub struct BoxPartView {
    path: String,
    part: Picked,
    title: NodeId,
    rows: Rows,
}

impl BoxPartView {
    /// Shows `part` of `model`, the box model at `path`, in `root`, which
    /// is emptied first. `None`, with nothing changed, where the model has
    /// no such part.
    pub fn build(ui: &mut Ui, root: NodeId, path: &str, model: &BoxModel, part: Picked) -> Option<Self> {
        let bone = model.bones.get(part.bone())?;
        let model_box = match part {
            Picked::Box(_, index) => Some(bone.boxes.get(index)?),
            Picked::Bone(_) => None,
        };
        for child in ui.children(root) {
            ui.remove(child);
        }
        let header = ui.panel(root, Style::column());
        ui.add_class(header, "inspector-header");
        let top = ui.panel(header, Style::row());
        ui.add_class(top, "header-row");
        let tile = ui.panel(top, Style::row());
        ui.add_class(tile, "obj-tile");
        ui.icon(tile, if model_box.is_some() { "cube" } else { "branch" });
        let title = ui.add_label(top, "");
        ui.add_class(title, "asset-name");

        let rows = match model_box {
            None => {
                let rows = part_rows(ui, root, "Bone", "branch");
                let row = prop(ui, rows, "Name");
                let name = ui.text_field(row, &bone.name);
                ui.add_class(name, "value-input");
                let row = prop(ui, rows, "Role");
                let labels = BoneRole::ALL.map(BoneRole::label);
                let chosen = BoneRole::ALL.iter().position(|role| *role == bone.role).unwrap_or(0);
                let role = ui.dropdown(row, &labels, chosen);
                let rows = part_rows(ui, root, "Transform", "transform");
                let pivot = vector_row(ui, rows, "Pivot").1;
                let rotation = vector_row(ui, rows, "Rotation").1;
                Rows::Bone { name, role, pivot, rotation }
            }
            Some(model_box) => {
                let rows = part_rows(ui, root, "Transform", "transform");
                let position = vector_row(ui, rows, "Position").1;
                let size = vector_row(ui, rows, "Size").1;
                let rotation = vector_row(ui, rows, "Rotation").1;
                let rows = part_rows(ui, root, "Faces", "texture");
                let unfolded = matches!(model_box.skin, BoxSkin::Unfolded { .. });
                let (sheet, mirror) = if unfolded {
                    let row = prop(ui, rows, "Sheet");
                    let vector = ui.panel(row, Style::default());
                    ui.add_class(vector, "vector");
                    let fields = ["x", "y"].map(|axis| {
                        let cell = ui.panel(vector, Style::row());
                        ui.add_class(cell, "axis-field");
                        let letter = ui.add_label(cell, &axis.to_uppercase());
                        ui.add_class(letter, "axis-letter");
                        ui.add_class(letter, "scrub");
                        let field = ui.text_field(cell, "");
                        ui.add_class(field, "axis-input");
                        field
                    });
                    let row = prop(ui, rows, "Mirror");
                    let mirror = ui.toggle(row, "", false);
                    (Some(fields), Some(mirror))
                } else {
                    let row = prop(ui, rows, "Sheet");
                    let note = ui.add_label(row, "A place for each face");
                    ui.add_class(note, "value");
                    (None, None)
                };
                let row = prop(ui, rows, "Inflate");
                let inflate = ui.text_field(row, "");
                ui.add_class(inflate, "value-input");
                ui.add_class(inflate, "number-input");
                Rows::Box { position, size, rotation, sheet, mirror, inflate }
            }
        };
        let view = Self { path: path.to_string(), part, title, rows };
        view.follow(ui, model);
        Some(view)
    }

    /// The part shown, with its model's file.
    pub fn shown(&self) -> (&str, Picked) {
        (&self.path, self.part)
    }

    /// The file its model is in changed its name or place.
    pub fn moved_to(&mut self, path: &str) {
        self.path = path.to_string();
    }

    /// Whether the model still has the part, as the kind of part the rows
    /// were built for.
    pub fn fits(&self, model: &BoxModel) -> bool {
        let Some(bone) = model.bones.get(self.part.bone()) else { return false };
        match (self.part, &self.rows) {
            (Picked::Bone(_), Rows::Bone { .. }) => true,
            (Picked::Box(_, index), Rows::Box { sheet, .. }) => bone
                .boxes
                .get(index)
                .is_some_and(|model_box| matches!(model_box.skin, BoxSkin::Unfolded { .. }) == sheet.is_some()),
            _ => false,
        }
    }

    /// What was typed or dragged in a row this frame, as a change to the
    /// part; with none, the rows are written from `model`, so they follow
    /// what changes it elsewhere.
    pub fn update(&mut self, ui: &mut Ui, model: &BoxModel) -> Option<BoxPartEdit> {
        let edit = self.read(ui, model);
        if edit.is_none() {
            self.follow(ui, model);
        }
        edit
    }

    fn read(&self, ui: &Ui, model: &BoxModel) -> Option<BoxPartEdit> {
        let bone = model.bones.get(self.part.bone())?;
        let vector = |fields: &[NodeId; 3], now: Vec3| {
            let read = read_numbers(ui, fields, &now.to_array());
            let read = Vec3::new(read[0], read[1], read[2]);
            (read != now && read.is_finite()).then_some(read)
        };
        match (&self.rows, self.part) {
            (Rows::Bone { name, role, pivot, rotation }, _) => {
                let typed = ui.text(*name).trim().to_string();
                if ui.submitted(*name) && !typed.is_empty() && typed != bone.name {
                    return Some(BoxPartEdit::Name(typed));
                }
                let chosen = BoneRole::ALL.get(ui.selected_option(*role)).copied().unwrap_or_default();
                if chosen != bone.role {
                    return Some(BoxPartEdit::Role(chosen));
                }
                let pivot = vector(pivot, bone.pivot).map(BoxPartEdit::Pivot);
                pivot.or_else(|| vector(rotation, bone.rotation).map(BoxPartEdit::BoneRotation))
            }
            (Rows::Box { position, size, rotation, sheet, mirror, inflate }, Picked::Box(_, index)) => {
                let model_box = bone.boxes.get(index)?;
                if let Some(at) = vector(position, model_box.from) {
                    return Some(BoxPartEdit::Position(at));
                }
                let whole = |pixels: f32| pixels.round().clamp(0.0, 1024.0) as u32;
                let sized = Vec3::from(model_box.size.map(|side| side as f32));
                if let Some(typed) = vector(size, sized).map(|typed| typed.to_array().map(whole))
                    && typed != model_box.size
                {
                    return Some(BoxPartEdit::Size(typed));
                }
                if let Some(turned) = vector(rotation, model_box.rotation) {
                    return Some(BoxPartEdit::BoxRotation(turned));
                }
                if let (Some(fields), BoxSkin::Unfolded { at, mirror: mirrored }) = (sheet, model_box.skin) {
                    let read = read_numbers(ui, fields, &at.map(|side| side as f32));
                    let typed = [whole(read[0]), whole(read[1])];
                    if typed != at {
                        return Some(BoxPartEdit::Sheet(typed));
                    }
                    if let Some(toggle) = mirror.filter(|toggle| ui.is_checked(*toggle) != mirrored) {
                        return Some(BoxPartEdit::Mirror(ui.is_checked(toggle)));
                    }
                }
                let read = read_numbers(ui, &[*inflate], &[model_box.inflate])[0];
                (read != model_box.inflate && read.is_finite()).then_some(BoxPartEdit::Inflate(read))
            }
            _ => None,
        }
    }

    /// Writes the rows from `model`, but for a field being typed in.
    fn follow(&self, ui: &mut Ui, model: &BoxModel) {
        let Some(bone) = model.bones.get(self.part.bone()) else { return };
        let title = match self.part {
            Picked::Bone(_) => bone.name.clone(),
            Picked::Box(_, index) => format!("{} ▸ Box {}", bone.name, index + 1),
        };
        if ui.text(self.title) != title {
            ui.set_text(self.title, &title);
        }
        match (&self.rows, self.part) {
            (Rows::Bone { name, role, pivot, rotation }, _) => {
                if ui.focused() != Some(*name) && ui.text(*name) != bone.name {
                    ui.set_text(*name, &bone.name);
                }
                let chosen = BoneRole::ALL.iter().position(|other| *other == bone.role).unwrap_or(0);
                if ui.selected_option(*role) != chosen {
                    ui.set_selected_option(*role, chosen);
                }
                write_numbers(ui, pivot, &bone.pivot.to_array());
                write_numbers(ui, rotation, &bone.rotation.to_array());
            }
            (Rows::Box { position, size, rotation, sheet, mirror, inflate }, Picked::Box(_, index)) => {
                let Some(model_box) = bone.boxes.get(index) else { return };
                write_numbers(ui, position, &model_box.from.to_array());
                write_numbers(ui, size, &model_box.size.map(|side| side as f32));
                write_numbers(ui, rotation, &model_box.rotation.to_array());
                if let (Some(fields), BoxSkin::Unfolded { at, mirror: mirrored }) = (sheet, model_box.skin) {
                    write_numbers(ui, fields, &at.map(|side| side as f32));
                    if let Some(toggle) = mirror.filter(|toggle| ui.is_checked(*toggle) != mirrored) {
                        ui.set_checked(toggle, mirrored);
                    }
                }
                write_numbers(ui, &[*inflate], &[model_box.inflate]);
            }
            _ => {}
        }
    }

    /// A row's fields, by its label's place among the vectors, for a test
    /// to type in: Pivot or Position, then for a box Size, then Rotation.
    pub fn vector_fields(&self, row: usize) -> Option<[NodeId; 3]> {
        match &self.rows {
            Rows::Bone { pivot, rotation, .. } => [*pivot, *rotation].get(row).copied(),
            Rows::Box { position, size, rotation, .. } => [*position, *size, *rotation].get(row).copied(),
        }
    }

    /// The field a bone's name is typed in.
    pub fn name_field(&self) -> Option<NodeId> {
        match &self.rows {
            Rows::Bone { name, .. } => Some(*name),
            Rows::Box { .. } => None,
        }
    }
}

/// A section's rows, under its heading.
fn part_rows(ui: &mut Ui, root: NodeId, title: &str, icon: &str) -> NodeId {
    part(ui, root, title, icon)
}
