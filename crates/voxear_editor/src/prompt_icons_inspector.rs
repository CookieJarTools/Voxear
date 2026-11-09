//! Prompt Icons in the Inspector: a section for each kind of device, closed
//! until opened, with a row for each of its controls, named as the device
//! marks it. Each row's picture is picked from the object picker, or taken
//! from a picture dropped from the Project panel; None takes it out, so the
//! control is drawn as Voxear draws it. A part of a sheet of icons is set in
//! the file, as `rect` (see `voxear_engine::prompt_icons`).

use voxear_assets::AssetDb;
use voxear_engine::prompt_icons::{IconDevice, PromptIconsFile};
use voxear_input::{Family, layouts};
use voxear_ui::{DragSource, NodeId, Ui};

use crate::asset_inspector::{Slot, show_slot, slot};

/// A control's row: its device and its name there, as `South`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconSlot {
    pub device: IconDevice,
    pub control: String,
}

/// What was done to a set's rows this frame.
pub enum IconsChange {
    /// A row's arrow clicked, under this node.
    Browse(IconSlot, NodeId),
    /// Something dropped onto a row.
    Drop(IconSlot, DragSource),
}

/// A set's rows.
pub struct PromptIconsFields {
    rows: Vec<(IconSlot, Slot)>,
}

impl PromptIconsFields {
    pub fn new(ui: &mut Ui, root: NodeId, icons: PromptIconsFile, db: &AssetDb) -> Self {
        let mut rows = Vec::new();
        for device in IconDevice::ALL {
            let section = section(ui, root, device);
            for (control, label) in controls(device) {
                let row = slot(ui, section, &label, "texture");
                let texture = icons.sprites(device).get(&control).map(|sprite| sprite.texture.as_str());
                let path = texture.and_then(|texture| texture_path(db, texture));
                show_slot(ui, row, path.as_deref(), "Picture");
                rows.push((IconSlot { device, control }, row));
            }
        }
        Self { rows }
    }

    /// What was asked of the rows this frame: a picker opened, or a picture
    /// dropped.
    pub fn update(&mut self, ui: &mut Ui) -> Option<IconsChange> {
        for (icon, row) in &self.rows {
            if let Some(source) = ui.dropped_onto(row.field) {
                return Some(IconsChange::Drop(icon.clone(), source));
            }
            if ui.clicked(row.arrow) || ui.clicked(row.field) {
                return Some(IconsChange::Browse(icon.clone(), row.field));
            }
        }
        None
    }
}

/// A device's section, closed, its rows going under it.
fn section(ui: &mut Ui, root: NodeId, device: IconDevice) -> NodeId {
    let icon = match device {
        IconDevice::Keyboard => "keyboard",
        IconDevice::Mouse => "mouse",
        _ => "gamepad",
    };
    let line = ui.panel(root, voxear_ui::Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, device.title(), false);
    ui.add_class(foldout, "section");
    ui.set_foldout_icon(foldout, icon);
    ui.foldout_content(foldout).expect("a foldout has content")
}

/// A texture's path, from the GUID or path a set names it by.
fn texture_path(db: &AssetDb, texture: &str) -> Option<String> {
    match texture.parse() {
        Ok(guid) => db.get(guid).map(|entry| entry.path.clone()),
        Err(_) => Some(texture.to_string()),
    }
}

/// A device's controls, each by its name in a path and as the device marks
/// it: every key and mouse button, and each of a pad's controls, its
/// sticks' directions and presses among them.
pub fn controls(device: IconDevice) -> Vec<(String, String)> {
    let (layout, prefix, family) = match device {
        IconDevice::Keyboard => (layouts::keyboard_mouse(), "Keyboard/", None),
        IconDevice::Mouse => (layouts::keyboard_mouse(), "Mouse/", None),
        IconDevice::Xbox => (layouts::gamepad(), "Gamepad/", Some(Family::Xbox)),
        IconDevice::PlayStation => (layouts::gamepad(), "Gamepad/", Some(Family::PlayStation)),
        IconDevice::Switch => (layouts::gamepad(), "Gamepad/", Some(Family::Switch)),
        IconDevice::Steam => (layouts::gamepad(), "Gamepad/", Some(Family::Steam)),
    };
    let mut found = Vec::new();
    for control in &layout.controls {
        let paths = control.paths.iter().filter_map(|path| path.strip_prefix(prefix));
        // A key once, as its first path names it; each of a pad's paths, as its sticks have several.
        let paths: Vec<&str> = if family.is_some() { paths.collect() } else { paths.take(1).collect() };
        for name in paths {
            let label = match family {
                Some(family) => pad_label(family, name),
                None if control.label.is_empty() => name.to_string(),
                None => control.label.clone(),
            };
            found.push((name.to_string(), label));
        }
    }
    found
}

/// A pad's control as that kind of pad marks it, with its name when the
/// mark is a picture rather than text, as a PlayStation pad's cross.
fn pad_label(family: Family, name: &str) -> String {
    let glyph = voxear_input::glyphs::glyph(family, &format!("Gamepad/{name}"), &|key| key.to_string());
    match glyph.text.is_empty() || glyph.text == name {
        true => name.to_string(),
        false => format!("{name} ({})", glyph.text),
    }
}
