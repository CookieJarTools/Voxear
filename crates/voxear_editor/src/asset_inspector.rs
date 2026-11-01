//! The Inspector for what is selected in the Project panel: its icon, name
//! and kind, where it is (path, GUID and size), and what its kind holds. A
//! material's colour, metallic, roughness and emission are edited in place
//! and saved to its file, showing at once where it is drawn. A script shows
//! the fields of the behaviour it declares, with their starting values, as
//! its last build describes them. A prefab and a scene list their Objs, and
//! a level its scenes. Levels, scenes, prefabs and scripts have an Open
//! button; a prefab opens on its own, to edit. A material, prefab or model
//! shows a large preview of itself under its name, which a drag turns.

use voxear_assets::AssetKind;
use voxear_engine::{Game, MaterialData, script_types::ScriptTypes};
use voxear_ui::{NodeId, PickedColor, Style, Ui};

use crate::{
    inspector::{color_row, number, prop},
    project_view::{ItemKind, ProjectItem},
};

/// What the asset view asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum AssetAction {
    Open(ProjectItem),
    SaveMaterial(String, MaterialData),
}

pub struct AssetView {
    item: ProjectItem,
    open: Option<NodeId>,
    material: Option<MaterialFields>,
    /// The preview's frame, which takes drags, and its image.
    preview: Option<(NodeId, NodeId)>,
}

/// A material's fields, and the values they were last saved with.
struct MaterialFields {
    data: MaterialData,
    color: [NodeId; 3],
    metallic: NodeId,
    roughness: NodeId,
    emissive: [NodeId; 3],
    /// The colour's and the emission's swatches, which open the colour picker.
    swatches: [NodeId; 2],
}

/// Which of a material's colours: its colour, picked as sRGB, or its
/// emission, an HDR colour kept as linear light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialColor {
    Base,
    Emission,
}

impl AssetView {
    /// Shows `item` in `root`, which is emptied first.
    pub fn build(ui: &mut Ui, root: NodeId, item: &ProjectItem, game: &Game, types: &ScriptTypes) -> Self {
        for child in ui.children(root) {
            ui.remove(child);
        }
        header(ui, root, item);
        let previewed =
            matches!(item.kind, ItemKind::Asset(AssetKind::Material | AssetKind::Prefab | AssetKind::Model));
        let preview = previewed.then(|| {
            let frame = ui.panel(root, Style::default());
            ui.add_class(frame, "asset-preview");
            let image = ui.image(frame);
            ui.add_class(image, "asset-preview-image");
            ui.set_visible(image, false);
            let hint = ui.add_label(frame, "Drag to turn");
            ui.add_class(hint, "asset-preview-hint");
            (frame, image)
        });
        let rows = part(ui, root, "Asset", "info");
        let guid = game.content.db().guid_of(&item.path).map(|g| g.to_string());
        let mut info = vec![("Kind", kind_name(item.kind).to_string())];
        if !item.path.is_empty() {
            info.push(("Path", item.path.clone()));
        }
        if let Some(guid) = guid {
            info.push(("GUID", guid));
        }
        if item.kind != ItemKind::Folder {
            info.push(("Size", size(item.size)));
        }
        for (label, value) in info {
            text_row(ui, rows, label, &value);
        }

        let file = game.project.root.join(&item.path);
        let mut view = Self { item: item.clone(), open: None, material: None, preview };
        match item.kind {
            ItemKind::Asset(AssetKind::Material) => view.material = material(ui, root, &file),
            ItemKind::Script => script(ui, root, item, types),
            ItemKind::Asset(AssetKind::Prefab) => objs(ui, root, "Objs", &file),
            ItemKind::Asset(AssetKind::Scene) => objs(ui, root, "Objs", &file),
            ItemKind::Asset(AssetKind::Level) => scenes(ui, root, &file, game),
            _ => {}
        }
        let opens = matches!(
            item.kind,
            ItemKind::Script | ItemKind::Asset(AssetKind::Level | AssetKind::Scene | AssetKind::Prefab)
        );
        if opens {
            let label = if item.kind == ItemKind::Script { "Open in Code Editor" } else { "Open" };
            let button = ui.button(root, label);
            ui.add_class(button, "add-component");
            view.open = Some(button);
        }
        view
    }

    /// What was asked of the editor this frame: Open, or a material changed.
    pub fn update(&mut self, ui: &Ui) -> Option<AssetAction> {
        if self.open.is_some_and(|button| ui.clicked(button)) {
            return Some(AssetAction::Open(self.item.clone()));
        }
        let fields = self.material.as_mut()?;
        let changed = fields.color.iter().chain(&fields.emissive).chain([&fields.metallic, &fields.roughness]);
        if !changed.into_iter().any(|&field| ui.changed(field)) {
            return None;
        }
        let read = |field: NodeId, was: f32| ui.text(field).trim().parse::<f32>().unwrap_or(was);
        let data = &mut fields.data;
        for (i, field) in fields.color.iter().enumerate() {
            data.color[i] = read(*field, data.color[i]).clamp(0.0, 1.0);
        }
        for (i, field) in fields.emissive.iter().enumerate() {
            data.emissive[i] = read(*field, data.emissive[i]).max(0.0);
        }
        data.metallic = read(fields.metallic, data.metallic).clamp(0.0, 1.0);
        data.roughness = read(fields.roughness, data.roughness).clamp(0.0, 1.0);
        Some(AssetAction::SaveMaterial(self.item.path.clone(), *data))
    }

    /// The material colour whose swatch was clicked, with the swatch and the
    /// colour as the picker shows it.
    pub fn color_clicked(&self, ui: &Ui) -> Option<(MaterialColor, NodeId, PickedColor)> {
        let fields = self.material.as_ref()?;
        let which = [MaterialColor::Base, MaterialColor::Emission];
        let (which, swatch) = which.into_iter().zip(fields.swatches).find(|&(_, swatch)| ui.clicked(swatch))?;
        Some((which, swatch, fields.picked(which)))
    }

    /// Sets a material colour from the colour picker, in its fields and
    /// swatch, and asks for the material to be saved.
    pub fn set_color(&mut self, ui: &mut Ui, which: MaterialColor, color: PickedColor) -> Option<AssetAction> {
        let fields = self.material.as_mut()?;
        let (values, targets) = match which {
            MaterialColor::Base => (color.srgb, &mut fields.data.color),
            MaterialColor::Emission => (color.to_linear(), &mut fields.data.emissive),
        };
        *targets = values;
        let shown = if which == MaterialColor::Base { fields.color } else { fields.emissive };
        for (field, value) in shown.into_iter().zip(values) {
            ui.set_text(field, &number(value));
        }
        fields.show_swatches(ui);
        Some(AssetAction::SaveMaterial(self.item.path.clone(), fields.data))
    }

    /// Shows the material's colours in their swatches again, as after typing.
    pub fn show_swatches(&self, ui: &mut Ui) {
        if let Some(fields) = &self.material {
            fields.show_swatches(ui);
        }
    }

    /// The large preview's frame, which takes drags to turn it, and its
    /// image, for an asset that has one.
    pub fn preview(&self) -> Option<(NodeId, NodeId)> {
        self.preview
    }

    pub fn item(&self) -> &ProjectItem {
        &self.item
    }
}

/// The asset's icon and name over its sections.
fn header(ui: &mut Ui, root: NodeId, item: &ProjectItem) {
    let header = ui.panel(root, Style::column());
    ui.add_class(header, "inspector-header");
    let top = ui.panel(header, Style::row());
    ui.add_class(top, "header-row");
    let tile = ui.panel(top, Style::row());
    ui.add_class(tile, "obj-tile");
    ui.icon(tile, icon(item.kind));
    let name = ui.add_label(top, if item.name.is_empty() { "Project" } else { &item.name });
    ui.add_class(name, "asset-name");
}

/// A section, headed by its icon and title, returning where its rows go.
fn part(ui: &mut Ui, root: NodeId, title: &str, icon: &str) -> NodeId {
    let line = ui.panel(root, Style::default());
    ui.add_class(line, "divider");
    let foldout = ui.foldout(root, title, true);
    ui.add_class(foldout, "section");
    ui.set_foldout_icon(foldout, icon);
    ui.foldout_content(foldout).expect("a foldout has content")
}

fn text_row(ui: &mut Ui, rows: NodeId, label: &str, value: &str) {
    let row = prop(ui, rows, label);
    let value = ui.add_label(row, value);
    ui.add_class(value, "value");
}

/// A material's values, to edit.
fn material(ui: &mut Ui, root: NodeId, file: &std::path::Path) -> Option<MaterialFields> {
    let text = std::fs::read_to_string(file).ok()?;
    let data: MaterialData = toml::from_str(&text).ok()?;
    let rows = part(ui, root, "Material", "material");
    let (color, color_swatch) = color_row(ui, rows, "Colour");
    let (emissive, emission_swatch) = color_row(ui, rows, "Emission");
    let one = |ui: &mut Ui, label: &str, value: f32| {
        let row = prop(ui, rows, label);
        let field = ui.text_field(row, &number(value));
        ui.add_class(field, "value-input");
        ui.add_class(field, "number-input");
        field
    };
    let (metallic, roughness) = (one(ui, "Metallic", data.metallic), one(ui, "Roughness", data.roughness));
    for (fields, values) in [(&color, data.color), (&emissive, data.emissive)] {
        for (field, value) in fields.iter().zip(values) {
            ui.set_text(*field, &number(value));
        }
    }
    let fields =
        MaterialFields { data, color, metallic, roughness, emissive, swatches: [color_swatch, emission_swatch] };
    fields.show_swatches(ui);
    Some(fields)
}

impl MaterialFields {
    /// A colour as the picker edits it: the colour as it is, the emission
    /// split into a colour and its intensity.
    fn picked(&self, which: MaterialColor) -> PickedColor {
        match which {
            MaterialColor::Base => PickedColor::srgb(self.data.color),
            MaterialColor::Emission => PickedColor::from_linear(self.data.emissive),
        }
    }

    fn show_swatches(&self, ui: &mut Ui) {
        for (which, swatch) in [MaterialColor::Base, MaterialColor::Emission].into_iter().zip(self.swatches) {
            ui.set_inline_style(swatch, &format!("background-color: {}", self.picked(which).hex()));
        }
    }
}

/// The behaviour a script declares, by its file's name, with its fields and
/// what they start as.
fn script(ui: &mut Ui, root: NodeId, item: &ProjectItem, types: &ScriptTypes) {
    let class = item.name.trim_end_matches(".cs");
    let Some(behaviour) = types.behaviours.iter().find(|b| b.short_name() == class) else {
        let rows = part(ui, root, "Script", "code");
        text_row(ui, rows, "Behaviour", "None that the last build describes");
        return;
    };
    let rows = part(ui, root, &behaviour.name, "code");
    if behaviour.fields.is_empty() {
        text_row(ui, rows, "Fields", "None");
    }
    for field in &behaviour.fields {
        let value = field.default.as_ref().map_or("None".to_string(), |v| v.to_string());
        text_row(ui, rows, &crate::props::nice_name(&field.name), &value);
    }
}

/// The Objs a prefab or scene holds, by name.
fn objs(ui: &mut Ui, root: NodeId, title: &str, file: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(file) else { return };
    let Ok(table) = text.parse::<toml::Table>() else { return };
    let names: Vec<String> = table
        .get("obj")
        .and_then(|objs| objs.as_array())
        .map(|objs| objs.iter().filter_map(|o| o.get("name")?.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let rows = part(ui, root, &format!("{title} · {}", names.len()), "obj");
    for name in names.iter().take(64) {
        let row = ui.add_label(rows, name);
        ui.add_class(row, "asset-line");
    }
}

/// The scenes a level loads, by path.
fn scenes(ui: &mut Ui, root: NodeId, file: &std::path::Path, game: &Game) {
    let Ok(text) = std::fs::read_to_string(file) else { return };
    let Ok(table) = text.parse::<toml::Table>() else { return };
    let rows = part(ui, root, "Scenes", "scene");
    if let Some(base) = table.get("stands_on").and_then(|v| v.as_str()) {
        text_row(ui, rows, "Stands on", &asset_path(game, base));
    }
    let listed = table.get("scene").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    for scene in listed.iter().filter_map(|s| s.get("scene")?.as_str()) {
        let row = ui.add_label(rows, &asset_path(game, scene));
        ui.add_class(row, "asset-line");
    }
}

/// A GUID as the path it names, or a path as it is.
fn asset_path(game: &Game, reference: &str) -> String {
    reference
        .parse()
        .ok()
        .and_then(|guid| game.content.db().path(guid))
        .map_or_else(|| reference.to_string(), |path| path.display().to_string())
}

fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Folder => "Folder",
        ItemKind::Script => "C# script",
        ItemKind::Asset(AssetKind::Level) => "Level",
        ItemKind::Asset(AssetKind::Scene) => "Scene",
        ItemKind::Asset(AssetKind::Prefab) => "Prefab",
        ItemKind::Asset(AssetKind::Model) => "Model",
        ItemKind::Asset(AssetKind::Material) => "Material",
        ItemKind::Asset(AssetKind::Texture) => "Texture",
        ItemKind::Asset(AssetKind::Ui) => "UI document",
    }
}

fn icon(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Folder => "folder",
        ItemKind::Script => "code",
        ItemKind::Asset(AssetKind::Level) => "level",
        ItemKind::Asset(AssetKind::Scene) => "scene",
        ItemKind::Asset(AssetKind::Prefab) => "cube",
        ItemKind::Asset(AssetKind::Model) => "box",
        ItemKind::Asset(AssetKind::Material) => "material",
        ItemKind::Asset(AssetKind::Texture) => "texture",
        ItemKind::Asset(AssetKind::Ui) => "panels",
    }
}

/// A size as file managers write it.
fn size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}
