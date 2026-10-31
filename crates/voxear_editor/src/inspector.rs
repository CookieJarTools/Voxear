//! The Inspector: the selected Obj's header (its kind, name, and the
//! active, tag, layer and static settings to come), then a section for its
//! Transform and for each component, divided by lines. Typed values apply
//! as they are typed, and values that change elsewhere show here, except in
//! the field being edited.

use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::EditBehaviours;
use voxear_render::{Camera, Light, LightKind, MeshRenderer};
use voxear_scene::{Name, Transform};
use voxear_ui::{NodeId, Style, Ui};

/// The fields showing an Obj's name and Transform, by what they edit.
struct Fields {
    name: NodeId,
    position: [NodeId; 3],
    rotation: [NodeId; 3],
    scale: [NodeId; 3],
}

pub struct Inspector {
    root: NodeId,
    shown: Option<Entity>,
    fields: Option<Fields>,
}

impl Inspector {
    pub fn new(root: NodeId) -> Self {
        Self { root, shown: None, fields: None }
    }

    /// Shows `entity`, or a hint when nothing is selected.
    pub fn show(&mut self, ui: &mut Ui, world: &World, entity: Option<Entity>) {
        for child in ui.children(self.root) {
            ui.remove(child);
        }
        self.shown = entity.filter(|&e| world.get::<Transform>(e).is_some());
        self.fields = None;
        let Some(entity) = self.shown else {
            let hint = ui.add_label(self.root, "Select an Obj in the Hierarchy to see it here.");
            ui.add_class(hint, "placeholder");
            return;
        };
        let name = header(ui, self.root, world, entity);

        let rows = section(ui, self.root, "Transform");
        let position = vector_row(ui, rows, "Position");
        let rotation = vector_row(ui, rows, "Rotation");
        let scale = vector_row(ui, rows, "Scale");
        self.fields = Some(Fields { name, position, rotation, scale });

        if let Some(mesh) = world.get::<MeshRenderer>(entity) {
            let rows = section(ui, self.root, "Mesh Renderer");
            pair(ui, rows, "Mesh", &format!("#{}", mesh.mesh.0));
            pair(ui, rows, "Material", &format!("#{}", mesh.material.0));
        }
        if let Some(light) = world.get::<Light>(entity) {
            let rows = section(ui, self.root, "Light");
            let kind = match light.kind {
                LightKind::Directional => "Directional",
                LightKind::Point => "Point",
                LightKind::Spot => "Spot",
            };
            pair(ui, rows, "Type", kind);
            pair(ui, rows, "Colour", &format!("{:.2}, {:.2}, {:.2}", light.color.x, light.color.y, light.color.z));
            pair(ui, rows, "Intensity", &number(light.intensity));
            if light.kind != LightKind::Directional {
                pair(ui, rows, "Range", &number(light.range));
            }
        }
        if let Some(camera) = world.get::<Camera>(entity) {
            let rows = section(ui, self.root, "Camera");
            pair(ui, rows, "Field of view", &format!("{}°", number(camera.fov_y_degrees)));
            pair(ui, rows, "Near", &number(camera.near));
        }
        for behaviour in world.get::<EditBehaviours>(entity).map(|b| b.0.clone()).unwrap_or_default() {
            let title = behaviour.type_name.rsplit('.').next().unwrap_or(&behaviour.type_name).to_string();
            let rows = section(ui, self.root, &title);
            if behaviour.fields.is_empty() {
                pair(ui, rows, "Script", &behaviour.type_name);
            }
            for (field, value) in behaviour.fields.iter() {
                pair(ui, rows, field, &value.to_string());
            }
        }
        let add = ui.button(self.root, "Add Component");
        ui.add_class(add, "add-component");
    }

    /// Applies what was typed, and shows changes made elsewhere.
    pub fn update(&mut self, ui: &mut Ui, world: &mut World) {
        let (Some(entity), Some(fields)) = (self.shown, &self.fields) else { return };
        let Some(&transform) = world.get::<Transform>(entity) else {
            self.show(ui, world, None);
            return;
        };
        if ui.changed(fields.name) {
            let name = ui.text(fields.name);
            if let Some(current) = world.get_mut::<Name>(entity) {
                current.0 = name;
            }
        }
        let (y, x, z) = transform.rotation.to_euler(EulerRot::YXZ);
        let euler = Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI);
        let mut edited = transform;
        edited.position = read(ui, &fields.position, transform.position);
        let rotation = read(ui, &fields.rotation, euler);
        if rotation != euler {
            let r = rotation * (std::f32::consts::PI / 180.0);
            edited.rotation = Quat::from_euler(EulerRot::YXZ, r.y, r.x, r.z);
        }
        edited.scale = read(ui, &fields.scale, transform.scale);
        if edited != transform
            && let Some(current) = world.get_mut::<Transform>(entity)
        {
            *current = edited;
        }
        write(ui, &fields.position, edited.position);
        write(ui, &fields.rotation, if rotation != euler { rotation } else { euler });
        write(ui, &fields.scale, edited.scale);
    }
}

/// The Obj's icon, its name field, whether it is static and whether it is
/// active, then its tag and layer. Returns the name field. Static, active,
/// tag and layer show their defaults until Objs have them.
fn header(ui: &mut Ui, root: NodeId, world: &World, entity: Entity) -> NodeId {
    let header = ui.panel(root, Style::column());
    ui.add_class(header, "inspector-header");
    let top = ui.panel(header, Style::row());
    ui.add_class(top, "header-row");
    let tile = ui.panel(top, Style::row());
    ui.add_class(tile, "obj-tile");
    let kind = crate::hierarchy::icon(
        world.get::<Camera>(entity).is_some(),
        world.get::<Light>(entity).is_some(),
        world.get::<MeshRenderer>(entity).is_some(),
    );
    ui.icon(tile, kind);
    let name = ui.text_field(top, &world.get::<Name>(entity).map(|n| n.0.clone()).unwrap_or_default());
    ui.add_class(name, "obj-name");
    let fixed = ui.toggle(top, "Static", false);
    ui.add_class(fixed, "checkbox");
    ui.toggle(top, "", true);

    let meta = ui.panel(header, Style::default());
    ui.add_class(meta, "meta-row");
    for (label, choice) in [("Tag", "Untagged"), ("Layer", "Default")] {
        let label = ui.add_label(meta, label);
        ui.add_class(label, "meta-label");
        ui.dropdown(meta, &[choice], 0);
    }
    name
}

/// A row of a label and three axis fields, X, Y and Z, each a field with
/// its coloured letter inside.
fn vector_row(ui: &mut Ui, rows: NodeId, label: &str) -> [NodeId; 3] {
    let row = prop(ui, rows, label);
    let vector = ui.panel(row, Style::default());
    ui.add_class(vector, "vector");
    ["x", "y", "z"].map(|axis| {
        let cell = ui.panel(vector, Style::row());
        ui.add_class(cell, "axis-field");
        let letter = ui.add_label(cell, &axis.to_uppercase());
        ui.add_class(letter, "axis-letter");
        ui.add_class(letter, &format!("axis-{axis}"));
        let field = ui.text_field(cell, "");
        ui.add_class(field, "axis-input");
        field
    })
}

/// A component's section: a line above it, then a foldout holding its rows.
fn section(ui: &mut Ui, root: NodeId, title: &str) -> NodeId {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, title, true);
    ui.add_class(foldout, "section");
    ui.foldout_content(foldout).expect("a foldout has content")
}

/// A property's row: its label in the first 88 pixels, the value after.
fn prop(ui: &mut Ui, rows: NodeId, label: &str) -> NodeId {
    let row = ui.panel(rows, Style::default());
    ui.add_class(row, "prop");
    let name = ui.add_label(row, label);
    ui.add_class(name, "prop-label");
    row
}

fn pair(ui: &mut Ui, rows: NodeId, label: &str, value: &str) {
    let row = prop(ui, rows, label);
    let value = ui.add_label(row, value);
    ui.add_class(value, "value");
}

/// A number as the Inspector writes it: up to three decimals, no trailing zeros.
fn number(value: f32) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.to_string() }
}

/// What three axis fields say, keeping `current` for any that is not a number
/// or that the user has not changed.
fn read(ui: &Ui, fields: &[NodeId; 3], current: Vec3) -> Vec3 {
    let mut value = current.to_array();
    for (axis, field) in fields.iter().enumerate() {
        if ui.changed(*field)
            && let Ok(number) = ui.text(*field).trim().parse::<f32>()
        {
            value[axis] = number;
        }
    }
    Vec3::from_array(value)
}

/// Shows `value` in fields the user is not editing.
fn write(ui: &mut Ui, fields: &[NodeId; 3], value: Vec3) {
    for (field, value) in fields.iter().zip(value.to_array()) {
        let text = number(value);
        if ui.focused() != Some(*field) && ui.text(*field) != text {
            ui.set_text(*field, &text);
        }
    }
}
