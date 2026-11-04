//! The Inspector for what is selected in the Project panel: its icon, name
//! and kind, where it is (path, GUID and size), and what its kind holds. A
//! material's colour, metallic, roughness and emission, and the pictures its
//! maps read, are edited in place and saved to its file, showing at once
//! where it is drawn. A model lists its materials, each with the `.material`
//! asset it draws with in place of the file's, and extracts them into
//! `.material` assets to edit (see `editor::model_materials`). A script shows
//! the fields of the behaviour it declares, with their starting values, as
//! its last build describes them, then its code, coloured as a code editor
//! colours it, as a UI document shows its markup. A prefab and a scene list their Objs. A
//! level shows the level it stands on, a reference picked or dropped as a
//! scene is, and its own scenes, each a reference to a scene with whether
//! it is the active one, whether it is required, whether it loads again on
//! every level change and what it counts for while the level loads, and a
//! button to take it out. As an Obj's references do, a scene's field
//! pings the scene in the Project on a click, opens the object picker from
//! its arrow to put another in its place, and takes a scene dropped on it;
//! Add Scene opens the picker for a new one, and takes a scene dropped on it
//! too (see `level_files`).
//! Levels, scenes, prefabs and scripts have an Open button; a prefab opens
//! on its own, to edit. A material, prefab or model shows a large preview
//! of itself under its name, which a drag turns.
//!
//! A model, a picture and a sound show their import settings, as Unity's
//! importers do (see `import_fields`), and a picture its size in pixels. A
//! sound shows how long it is, its channels and sample rate, and a button
//! to listen to it, which plays it outside the game's volumes.

use std::path::PathBuf;

use voxear_assets::{AssetDb, AssetGuid, AssetKind, MaterialRemap};
use voxear_engine::{Game, MaterialData, MaterialMap, localization::StringTable, script_types::ScriptTypes};
use voxear_render::{Glow, Leaves};
use voxear_ui::{DragSource, NodeId, PickedColor, Style, Ui};

use crate::{
    collection,
    lighting_panel::{ColorClicked, LightingAction, LightingFields},
    import_fields::{ImportFields, ImportSettings},
    inspector::{color_row, number, prop},
    level_files::{self, LevelEdit},
    project_view::{ItemKind, ProjectItem},
    strings_inspector::{StringsChange, StringsFields},
};

/// How many lines of a file's code the Inspector shows.
const CODE_LINES: usize = 200;

/// What the asset view asks of the editor.
#[derive(Clone, Debug, PartialEq)]
pub enum AssetAction {
    Open(ProjectItem),
    /// A setting of the lighting settings shown, written into their file.
    Lighting(LightingAction),
    SaveMaterial(String, MaterialData),
    /// Change the scenes of the level at this path.
    EditLevel(String, LevelEdit),
    /// Write these import settings for the asset at this path, and import
    /// it, and what reads it, again.
    Import(String, ImportSettings),
    /// Open the object picker under `anchor` for the scene at this place in
    /// the level at this path, or for a new one.
    BrowseScene {
        level: String,
        at: Option<usize>,
        anchor: NodeId,
    },
    /// Something was dragged onto the scene at this place in the level at
    /// this path, or onto Add Scene.
    DropScene {
        level: String,
        at: Option<usize>,
        source: DragSource,
    },
    /// Open the object picker under `anchor` for the level the level at
    /// this path stands on.
    BrowseStandsOn {
        level: String,
        anchor: NodeId,
    },
    /// Something was dragged onto the level a level stands on.
    DropStandsOn {
        level: String,
        source: DragSource,
    },
    /// Bake the sound paths of the level at this path.
    BakePaths(String),
    /// Show where the asset at this path is in the Project panel.
    Ping(String),
    /// Play the sound at this path in the editor, or stop it.
    Listen(String),
    StopListening,
    /// Open the object picker under `anchor` for a picture for a map of the
    /// material at this path.
    BrowseMap {
        material: String,
        map: MaterialMap,
        anchor: NodeId,
    },
    /// Something was dragged onto a map of the material at this path.
    DropMap {
        material: String,
        map: MaterialMap,
        source: DragSource,
    },
    /// Open the object picker under `anchor` for the `.material` a part of
    /// the model at this path draws with.
    BrowseRemap {
        model: String,
        part: String,
        anchor: NodeId,
    },
    /// Something was dragged onto a model's material.
    DropRemap {
        model: String,
        part: String,
        source: DragSource,
    },
    /// Write the model's materials out as `.material` assets, and draw it
    /// with them.
    ExtractMaterials(String),
    /// Write a string table to the file at this path, and when its entries
    /// came, went or moved, show it again.
    SaveStrings {
        path: String,
        table: voxear_engine::localization::StringTable,
        rebuild: bool,
    },
    /// Say something in the status bar.
    Notice(String),
}

pub struct AssetView {
    item: ProjectItem,
    open: Option<NodeId>,
    material: Option<MaterialFields>,
    level: Option<LevelFields>,
    import: Option<ImportFields>,
    /// A model's materials, filled once the model is loaded.
    remaps: Option<ModelMaterials>,
    /// The preview's frame, which takes drags, and its image.
    preview: Option<(NodeId, NodeId)>,
    /// A sound's play button, and whether it shows that the sound plays.
    listen: Option<(Listen, bool)>,
    strings: Option<StringsFields>,
    /// A level's Bake Paths button.
    bake: Option<NodeId>,
    /// Lighting settings' every setting, changed in their file, and that file.
    lighting: Option<(LightingFields, PathBuf)>,
}

/// A sound's play button, with its icon and its words.
#[derive(Clone, Copy)]
struct Listen {
    button: NodeId,
    icon: NodeId,
    label: NodeId,
}

/// The level a level stands on, with its path, and its own scenes, in the
/// file's order, in a collection field whose items drag into a new order,
/// with Add Scene under it.
struct LevelFields {
    stands_on: (Slot, Option<String>),
    items: NodeId,
    scenes: Vec<SceneRow>,
    add: NodeId,
}

/// One of a level's scenes: its reference field and the field's arrow, the
/// scene's project path, its Active, Required and Reload boxes, its weight
/// with the value it was shown with, and its remove button.
struct SceneRow {
    field: NodeId,
    arrow: NodeId,
    path: String,
    active: NodeId,
    required: NodeId,
    reload: NodeId,
    weight: (NodeId, f32),
    remove: NodeId,
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
    /// Each map's picture, in [`MaterialMap::ALL`]'s order, with its path.
    maps: [(Slot, Option<String>); 4],
    foliage: NodeId,
    /// How much it bends in the wind, and its leaves flutter.
    wind: [NodeId; 2],
    /// What its leaves do through the year.
    leaves: NodeId,
    /// How its emission moves, and how fast.
    glow: NodeId,
    glow_speed: NodeId,
    glass: NodeId,
    ior: NodeId,
}

/// A field naming an asset, as an Obj's references show one: its icon and
/// name, which ping the asset, and an arrow for the object picker.
#[derive(Clone, Copy)]
struct Slot {
    field: NodeId,
    arrow: NodeId,
    icon: NodeId,
    label: NodeId,
}

/// A model's materials, each with the `.material` it draws with instead of
/// the file's, and Extract Materials.
struct ModelMaterials {
    rows: NodeId,
    /// Filled once the model is loaded, which names its materials.
    filled: bool,
    /// Each material's part, its slot and the remapped asset's path.
    parts: Vec<(String, Slot, Option<String>)>,
    extract: Option<NodeId>,
}

/// Which of a material's colours: its colour, picked as sRGB, or its
/// emission, an HDR colour kept as linear light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialColor {
    Base,
    Emission,
}

impl AssetView {
    /// The box of a level's scenes, whose items drag into a new order.
    pub fn level_items(&self) -> Option<NodeId> {
        self.level.as_ref().map(|level| level.items)
    }

    /// A string table's box of entries, when it shows one.
    pub fn strings_items(&self) -> Option<NodeId> {
        self.strings.as_ref().map(StringsFields::items)
    }

    /// Shows `item` in `root`, which is emptied first.
    pub fn build(ui: &mut Ui, root: NodeId, item: &ProjectItem, game: &Game, types: &ScriptTypes, dark: bool) -> Self {
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
        if item.kind == ItemKind::Asset(AssetKind::Texture)
            && let Ok((width, height)) = image::image_dimensions(game.content.db().file(&item.path))
        {
            info.push(("Pixels", format!("{width} × {height}")));
        }
        for (label, value) in info {
            text_row(ui, rows, label, &value);
        }

        // In the project's folder or a package's, with its import settings in the `.meta` they are read from.
        let file = game.content.db().file(&item.path);
        let meta = game.content.db().mounts().meta_file(&item.path);
        let mut view = Self {
            item: item.clone(),
            open: None,
            material: None,
            level: None,
            import: None,
            remaps: None,
            preview,
            listen: None,
            strings: None,
            bake: None,
            lighting: None,
        };
        match item.kind {
            ItemKind::Asset(AssetKind::Material) => view.material = material(ui, root, &file, game.content.db()),
            ItemKind::Asset(AssetKind::Model) => {
                let rows = part(ui, root, "Import Settings", "box");
                view.import = Some(ImportFields::model(ui, rows, &meta));
                let rows = part(ui, root, "Materials", "material");
                view.remaps = Some(ModelMaterials { rows, filled: false, parts: Vec::new(), extract: None });
            }
            ItemKind::Asset(AssetKind::Texture) => {
                let rows = part(ui, root, "Import Settings", "texture");
                view.import = Some(ImportFields::texture(ui, rows, &meta));
            }
            ItemKind::Asset(AssetKind::Sound) => {
                view.listen = Some((sound(ui, root, &file), false));
                let rows = part(ui, root, "Import Settings", "audio");
                view.import = Some(ImportFields::sound(ui, rows, &meta));
            }
            ItemKind::Script => {
                script(ui, root, item, types);
                code(ui, root, &file, "csharp", dark);
            }
            ItemKind::Asset(AssetKind::Ui) => code(ui, root, &file, "vui", dark),
            ItemKind::Asset(AssetKind::AssetTable) => code(ui, root, &file, "toml", dark),
            // Every setting to change in place, a tab for each group, not the file's text.
            ItemKind::Asset(AssetKind::Lighting) => {
                let rows = part(ui, root, "Lighting", "light");
                let tabs = ui.panel(rows, Style::row());
                ui.add_class(tabs, "lighting-asset-tabs");
                let body = ui.panel(rows, Style::column());
                let mut fields = LightingFields::new(ui, tabs, body);
                fields.show_file(ui, &file);
                view.lighting = Some((fields, file.clone()));
            }
            ItemKind::Asset(AssetKind::Prefab) => objs(ui, root, "Objs", &file),
            ItemKind::Asset(AssetKind::Scene) => objs(ui, root, "Objs", &file),
            ItemKind::Asset(AssetKind::Level) => {
                view.level = scenes(ui, root, &file, game);
                view.bake = Some(sound_paths(ui, root, &item.path, game));
            }
            ItemKind::Asset(AssetKind::Strings) => {
                let read = std::fs::read_to_string(&file).map_err(anyhow::Error::from);
                match read.and_then(|text| StringTable::parse(&text)) {
                    Ok(table) => {
                        let languages = &game.project.file.localization.languages;
                        view.strings = Some(StringsFields::build(ui, root, table, languages, file.clone()));
                    }
                    Err(error) => {
                        text_row(ui, rows, "Error", &format!("{error:#}"));
                        code(ui, root, &file, "toml", dark);
                    }
                }
            }
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

    /// Only an asset's fields to change, a material's, for where it shows
    /// outside the Inspector, as under its picture on a VoxDocs page. None
    /// for an asset without them.
    pub fn fields(ui: &mut Ui, root: NodeId, item: &ProjectItem, game: &Game) -> Option<Self> {
        if item.kind != ItemKind::Asset(AssetKind::Material) {
            return None;
        }
        let material = material(ui, root, &game.content.db().file(&item.path), game.content.db())?;
        Some(Self {
            item: item.clone(),
            open: None,
            material: Some(material),
            level: None,
            import: None,
            remaps: None,
            preview: None,
            listen: None,
            strings: None,
            bake: None,
            lighting: None,
        })
    }

    /// What was asked of the editor this frame: Open, a material changed, a
    /// level's scenes, or a model's import settings applied.
    pub fn update(&mut self, ui: &mut Ui) -> Option<AssetAction> {
        if let Some(action) = self.lighting.as_mut().and_then(|(fields, _)| fields.update(ui)) {
            return Some(AssetAction::Lighting(action));
        }
        if self.open.is_some_and(|button| ui.clicked(button)) {
            return Some(AssetAction::Open(self.item.clone()));
        }
        if self.bake.is_some_and(|button| ui.clicked(button)) {
            return Some(AssetAction::BakePaths(self.item.path.clone()));
        }
        if let Some((listen, playing)) = self.listen
            && ui.clicked(listen.button)
        {
            return Some(if playing {
                AssetAction::StopListening
            } else {
                AssetAction::Listen(self.item.path.clone())
            });
        }
        if let Some(settings) = self.import.as_mut().and_then(|import| import.update(ui)) {
            return Some(AssetAction::Import(self.item.path.clone(), settings));
        }
        if let Some(action) = self.level.as_ref().and_then(|level| level.update(ui, &self.item.path)) {
            return Some(action);
        }
        if let Some(action) = self.remaps.as_ref().and_then(|remaps| remaps.update(ui, &self.item.path)) {
            return Some(action);
        }
        if let Some(strings) = self.strings.as_mut() {
            let path = self.item.path.clone();
            return match strings.update(ui)? {
                StringsChange::Typed => {
                    Some(AssetAction::SaveStrings { path, table: strings.table.clone(), rebuild: false })
                }
                StringsChange::Entries => {
                    Some(AssetAction::SaveStrings { path, table: strings.table.clone(), rebuild: true })
                }
                StringsChange::Refused(key) => Some(AssetAction::Notice(match key.is_empty() {
                    true => "A key cannot be empty".to_string(),
                    false => format!("Another entry has the key {key}"),
                })),
                StringsChange::Notice(notice) => Some(AssetAction::Notice(notice)),
            };
        }
        let fields = self.material.as_mut()?;
        for (map, (slot, path)) in MaterialMap::ALL.into_iter().zip(&fields.maps) {
            let material = self.item.path.clone();
            if let Some(source) = ui.dropped_onto(slot.field) {
                return Some(AssetAction::DropMap { material, map, source });
            }
            if ui.clicked(slot.arrow) {
                return Some(AssetAction::BrowseMap { material, map, anchor: slot.field });
            }
            if let Some(path) = path.clone().filter(|_| ui.clicked(slot.field)) {
                return Some(AssetAction::Ping(path));
            }
        }
        let changed = fields.color.iter().chain(&fields.emissive)
            .chain([&fields.metallic, &fields.roughness, &fields.foliage, &fields.leaves, &fields.glass, &fields.ior])
            .chain([&fields.glow, &fields.glow_speed])
            .chain(&fields.wind);
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
        let foliage = ui.is_checked(fields.foliage);
        // Foliage that turns on or off brings its own wind, unless the wind was changed this frame.
        let [bend, flutter] = fields.wind;
        if ui.changed(bend) {
            data.wind_bend = Some(read(bend, data.wind_bend()).clamp(0.0, 4.0));
        }
        if ui.changed(flutter) {
            data.wind_flutter = Some(read(flutter, data.wind_flutter()).clamp(0.0, 4.0));
        }
        data.foliage = foliage;
        data.leaves = Leaves::ALL[ui.selected_option(fields.leaves).min(Leaves::ALL.len() - 1)];
        data.glow = Glow::ALL[ui.selected_option(fields.glow).min(Glow::ALL.len() - 1)];
        data.glow_speed = read(fields.glow_speed, data.glow_speed).clamp(0.0, 30.0);
        data.glass = ui.is_checked(fields.glass);
        data.ior = read(fields.ior, data.ior).clamp(1.0, 3.0);
        let data = data.clone();
        fields.show_what_applies(ui);
        Some(AssetAction::SaveMaterial(self.item.path.clone(), data))
    }

    /// Sets the picture a map of the material shown reads, or none, and
    /// asks for the material to be saved.
    pub fn set_map(&mut self, ui: &mut Ui, map: MaterialMap, picture: Option<(AssetGuid, String)>) -> Option<AssetAction> {
        let fields = self.material.as_mut()?;
        let at = MaterialMap::ALL.iter().position(|&m| m == map)?;
        fields.data.set_map(map, picture.as_ref().map(|(guid, _)| *guid));
        let (slot, path) = &mut fields.maps[at];
        *path = picture.map(|(_, path)| path);
        show_slot(ui, *slot, path.as_deref(), "Texture");
        Some(AssetAction::SaveMaterial(self.item.path.clone(), fields.data.clone()))
    }

    /// Whether a model is shown whose materials wait for it to load.
    pub fn wants_materials(&self) -> bool {
        self.remaps.as_ref().is_some_and(|remaps| !remaps.filled)
    }

    /// Lists the model's materials, by part and name, each with the
    /// `.material` its `.meta` remaps it to, and Extract Materials.
    pub fn fill_materials(&mut self, ui: &mut Ui, materials: &[(String, String)], remap: &MaterialRemap, db: &AssetDb) {
        let Some(remaps) = self.remaps.as_mut() else { return };
        for child in ui.children(remaps.rows) {
            ui.remove(child);
        }
        remaps.parts.clear();
        for (number, (part, name)) in materials.iter().enumerate() {
            let label = if name.is_empty() { format!("Material {number}") } else { name.clone() };
            let path = remap.get(part).and_then(|guid| db.get(guid)).map(|entry| entry.path.clone());
            let slot = slot(ui, remaps.rows, &label, "material");
            show_slot(ui, slot, path.as_deref(), "Material");
            remaps.parts.push((part.clone(), slot, path));
        }
        if materials.is_empty() {
            text_row(ui, remaps.rows, "Materials", "None");
        } else {
            let extract = ui.button(remaps.rows, "Extract Materials");
            ui.add_class(extract, "add-component");
            remaps.extract = Some(extract);
        }
        remaps.filled = true;
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
        Some(AssetAction::SaveMaterial(self.item.path.clone(), fields.data.clone()))
    }

    /// Shows the material's colours in their swatches again, as after typing.
    pub fn show_swatches(&self, ui: &mut Ui) {
        if let Some(fields) = &self.material {
            fields.show_swatches(ui);
        }
    }

    /// The colour setting of lighting settings whose swatch was clicked.
    pub fn lighting_color_clicked(&self, ui: &Ui) -> Option<ColorClicked> {
        self.lighting.as_ref().and_then(|(fields, _)| fields.color_clicked(ui))
    }

    /// Shows lighting settings again as their file now has them, after a
    /// change to it.
    pub fn refresh_lighting(&mut self, ui: &mut Ui) {
        if let Some((fields, file)) = &mut self.lighting {
            fields.show_file(ui, file);
        }
    }

    /// Shows on a sound's button whether it is playing now: the editor plays
    /// one sound at a time, so another starting stops this one.
    pub fn show_listening(&mut self, ui: &mut Ui, playing: Option<&str>) {
        let Some((listen, shown)) = &mut self.listen else { return };
        let playing = playing == Some(self.item.path.as_str());
        if *shown != playing {
            *shown = playing;
            ui.set_text(listen.label, if playing { "Stop" } else { "Play" });
            ui.set_icon(listen.icon, if playing { "stop" } else { "play" });
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

/// Whether a level's sound paths are baked, and the button that bakes
/// them, which this returns.
fn sound_paths(ui: &mut Ui, root: NodeId, level: &str, game: &Game) -> NodeId {
    let rows = part(ui, root, "Sound Paths", "branch");
    let file = game.project.root.join(voxear_engine::acoustics::paths_file(level));
    let baked = std::fs::metadata(file).map(|meta| meta.len()).ok();
    text_row(
        ui,
        rows,
        "Paths",
        &baked.map_or_else(|| "Not baked".to_string(), |bytes| format!("Baked · {}", size(bytes))),
    );
    let button = ui.button(rows, "Bake Paths");
    ui.add_class(button, "add-component");
    button
}

/// What a sound file holds, and the button that plays it, which this
/// returns.
fn sound(ui: &mut Ui, root: NodeId, file: &std::path::Path) -> Listen {
    let rows = part(ui, root, "Sound", "audio");
    let extension = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let read = std::fs::read(file).map_err(anyhow::Error::from);
    match read.and_then(|bytes| voxear_engine::audio::probe(&bytes, extension.as_deref())) {
        Ok(info) => {
            text_row(ui, rows, "Length", &length(info.seconds()));
            let channels = match info.channels {
                1 => "Mono".to_string(),
                2 => "Stereo".to_string(),
                more => format!("{more}, played as stereo"),
            };
            text_row(ui, rows, "Channels", &channels);
            text_row(ui, rows, "Sample rate", &format!("{} Hz", info.sample_rate));
        }
        Err(error) => text_row(ui, rows, "Problem", &format!("{error:#}")),
    }
    let button = ui.icon_button(rows, "play");
    ui.remove_class(button, "icon-button");
    ui.add_class(button, "listen");
    ui.set_name(button, "listen");
    let icon = ui.children(button)[0];
    let label = ui.add_label(button, "Play");
    Listen { button, icon, label }
}

/// A length as players show one: `0:04.2`, or `2:01` past a minute.
fn length(seconds: f64) -> String {
    let whole = seconds.max(0.0);
    let minutes = (whole / 60.0).floor();
    match minutes {
        0.0 => format!("0:{:04.1}", whole),
        _ => format!("{}:{:02}", minutes as u64, (whole - minutes * 60.0).floor() as u64),
    }
}

/// A material's values, to edit.
fn material(ui: &mut Ui, root: NodeId, file: &std::path::Path, db: &AssetDb) -> Option<MaterialFields> {
    let text = std::fs::read_to_string(file).ok()?;
    let data: MaterialData = toml::from_str(&text).ok()?;
    let rows = part(ui, root, "Material", "material");
    let (color, color_swatch) = color_row(ui, rows, "Colour");
    let (emissive, emission_swatch) = color_row(ui, rows, "Emission");
    let row = prop(ui, rows, "Glow");
    let glow = ui.dropdown(row, &Glow::ALL.map(Glow::name), Glow::ALL.iter().position(|&g| g == data.glow).unwrap_or_default());
    ui.add_class(glow, "value-input");
    let one = |ui: &mut Ui, label: &str, value: f32| {
        let row = prop(ui, rows, label);
        let field = ui.text_field(row, &number(value));
        ui.add_class(field, "value-input");
        ui.add_class(field, "number-input");
        field
    };
    let glow_speed = one(ui, "Glow speed", data.glow_speed);
    let (metallic, roughness) = (one(ui, "Metallic", data.metallic), one(ui, "Roughness", data.roughness));
    for (fields, values) in [(&color, data.color), (&emissive, data.emissive)] {
        for (field, value) in fields.iter().zip(values) {
            ui.set_text(*field, &number(value));
        }
    }
    let labels = ["Colour Map", "Metallic Map", "Normal Map", "Emission Map"];
    let maps = MaterialMap::ALL.map(|map| {
        let at = MaterialMap::ALL.iter().position(|&m| m == map).unwrap_or_default();
        let slot = slot(ui, rows, labels[at], "texture");
        let path = data.map(map).and_then(|guid| db.get(guid)).map(|entry| entry.path.clone());
        show_slot(ui, slot, path.as_deref(), "Texture");
        (slot, path)
    });
    let row = prop(ui, rows, "Foliage");
    let foliage = ui.toggle(row, "", data.foliage);
    ui.add_class(foliage, "checkbox");
    let wind = [one(ui, "Wind bend", data.wind_bend()), one(ui, "Leaf flutter", data.wind_flutter())];
    let row = prop(ui, rows, "Leaves");
    let names = Leaves::ALL.map(Leaves::name);
    let leaves = ui.dropdown(row, &names, Leaves::ALL.iter().position(|&l| l == data.leaves).unwrap_or_default());
    ui.add_class(leaves, "value-input");
    let row = prop(ui, rows, "Glass");
    let glass = ui.toggle(row, "", data.glass);
    ui.add_class(glass, "checkbox");
    let ior = one(ui, "IOR", data.ior);
    let swatches = [color_swatch, emission_swatch];
    let fields =
        MaterialFields { data, color, metallic, roughness, emissive, swatches, maps, foliage, wind, leaves, glow, glow_speed, glass, ior };
    fields.show_swatches(ui);
    fields.show_what_applies(ui);
    Some(fields)
}

/// A row with a field naming an asset.
fn slot(ui: &mut Ui, rows: NodeId, label: &str, icon: &str) -> Slot {
    let row = prop(ui, rows, label);
    let field = ui.panel(row, Style::row());
    ui.add_class(field, "value-picker");
    ui.accept_drops(field, true);
    let icon = ui.icon(field, icon);
    ui.add_class(icon, "picker-icon");
    let label = ui.add_label(field, "");
    ui.add_class(label, "picker-label");
    let arrow = ui.icon_button(field, "chevron-down");
    ui.add_class(arrow, "picker-arrow");
    Slot { field, arrow, icon, label }
}

/// Shows the asset at `path` in a slot by its name, or none of `kind`.
fn show_slot(ui: &mut Ui, slot: Slot, path: Option<&str>, kind: &str) {
    let name = path.map(|path| path.rsplit('/').next().unwrap_or(path));
    let name = name.map(|name| name.rsplit_once('.').map_or(name, |(stem, _)| stem));
    ui.set_text(slot.label, &name.map_or_else(|| format!("None ({kind})"), str::to_string));
    ui.set_visible(slot.icon, path.is_some());
}

impl ModelMaterials {
    /// What was asked this frame of the model at `model`: a material's
    /// field clicked, its arrow, something dropped on it, or Extract
    /// Materials.
    fn update(&self, ui: &mut Ui, model: &str) -> Option<AssetAction> {
        let model = model.to_string();
        if self.extract.is_some_and(|button| ui.clicked(button)) {
            return Some(AssetAction::ExtractMaterials(model));
        }
        for (part, slot, path) in &self.parts {
            let part = part.clone();
            if let Some(source) = ui.dropped_onto(slot.field) {
                return Some(AssetAction::DropRemap { model, part, source });
            }
            if ui.clicked(slot.arrow) {
                return Some(AssetAction::BrowseRemap { model, part, anchor: slot.field });
            }
            if let Some(path) = path.clone().filter(|_| ui.clicked(slot.field)) {
                return Some(AssetAction::Ping(path));
            }
        }
        None
    }
}

impl MaterialFields {
    /// Hides the rows that do nothing as the material stands: glow speed
    /// for a steady glow, leaves for what is not foliage, and the index of
    /// refraction for what is not glass.
    fn show_what_applies(&self, ui: &mut Ui) {
        let rows = [
            (self.glow_speed, self.data.glow != Glow::Steady),
            (self.leaves, self.data.foliage),
            (self.ior, self.data.glass),
        ];
        for (field, applies) in rows {
            if let Some(row) = ui.parent(field) {
                ui.set_visible(row, applies);
            }
        }
    }

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

/// A file's text, as a code editor colours it: the first lines of it, with
/// how many more there are, so a long file does not make the Inspector slow.
fn code(ui: &mut Ui, root: NodeId, file: &std::path::Path, language: &str, dark: bool) {
    let Ok(text) = std::fs::read_to_string(file) else { return };
    let lines: Vec<&str> = text.lines().collect();
    let shown = lines[..lines.len().min(CODE_LINES)].join("\n");
    let rows = part(ui, root, "Code", "code");
    let code = ui.rich_label(
        rows,
        &voxear_docs::highlight::colored(language, &voxear_docs::highlight::expand_tabs(&shown), dark),
    );
    ui.add_class(code, "code-preview");
    if lines.len() > CODE_LINES {
        let more = ui.add_label(rows, &format!("… {} more lines", lines.len() - CODE_LINES));
        ui.add_class(more, "asset-line");
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

/// The level a level stands on, and its own scenes, to change.
fn scenes(ui: &mut Ui, root: NodeId, file: &std::path::Path, game: &Game) -> Option<LevelFields> {
    let text = std::fs::read_to_string(file).ok()?;
    let table = text.parse::<toml::Table>().ok()?;
    let entries = level_files::entries(&text).ok()?;
    let rows = part(ui, root, "Scenes", "scene");
    let stands_on = slot(ui, rows, "Stands on", "level");
    let base = table.get("stands_on").and_then(|v| v.as_str()).map(|base| asset_path(game, base));
    show_slot(ui, stands_on, base.as_deref(), "Level");
    let list = collection::collection(ui, rows, entries.len(), true, Some("Add Scene"));
    let mut scenes = Vec::new();
    for (place, entry) in entries.iter().enumerate() {
        let found = game.content.parse(&entry.scene).ok().and_then(|r| r.guid()).and_then(|g| game.content.db().get(g));
        let path = found.map_or_else(|| entry.scene.clone(), |found| found.path.clone());
        let item = collection::item(ui, list.items, place, true);
        let row = item.body;
        ui.add_class(row, "level-scene");
        let top = ui.panel(row, Style::row());
        ui.add_class(top, "level-scene-top");
        let field = ui.panel(top, Style::row());
        ui.add_class(field, "value-picker");
        ui.accept_drops(field, true);
        let icon = ui.icon(field, "scene");
        ui.add_class(icon, "picker-icon");
        let name = path.rsplit('/').next().unwrap_or(&path);
        let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let label = ui.add_label(field, &if found.is_some() { name.to_string() } else { format!("Missing ({name})") });
        ui.add_class(label, "picker-label");
        let arrow = ui.icon_button(field, "chevron-down");
        ui.add_class(arrow, "picker-arrow");
        let remove = item.remove.expect("a level's scenes can be taken out");
        let flags = ui.panel(row, Style::row());
        ui.add_class(flags, "level-scene-flags");
        let mut check = |label: &str, on: bool| {
            let toggle = ui.toggle(flags, label, on);
            ui.add_class(toggle, "checkbox");
            ui.add_class(toggle, "level-flag");
            toggle
        };
        let (active, required) = (check("Active", entry.active), check("Required", entry.required));
        let reload = check("Reload", entry.reload);
        let group = ui.panel(flags, Style::row());
        ui.add_class(group, "level-weight-group");
        let label = ui.add_label(group, "Weight");
        ui.add_class(label, "level-weight-label");
        let weight = ui.text_field(group, &number(entry.weight));
        for class in ["value-input", "number-input", "level-weight"] {
            ui.add_class(weight, class);
        }
        let weight = (weight, entry.weight);
        scenes.push(SceneRow { field, arrow, path, active, required, reload, weight, remove });
    }
    let add = list.add.expect("a level takes scenes");
    ui.accept_drops(add, true);
    let stands_on = (stands_on, base.filter(|_| found_base(game, &table)));
    Some(LevelFields { stands_on, items: list.items, scenes, add })
}

/// Whether the level a level's file says it stands on is in the project.
fn found_base(game: &Game, table: &toml::Table) -> bool {
    let Some(base) = table.get("stands_on").and_then(|v| v.as_str()) else { return false };
    let db = game.content.db();
    base.parse().ok().and_then(|guid| db.get(guid)).is_some() || db.guid_of(base).is_some()
}

impl LevelFields {
    /// What was asked this frame of the level at `level`: a scene's field
    /// clicked, its arrow, or something dropped on it; Add Scene clicked or
    /// dropped on; a box ticked, or a scene taken out. The active box is
    /// ticked on another scene to move it; ticked off, it stays, as one
    /// scene is always the active one.
    fn update(&self, ui: &mut Ui, level: &str) -> Option<AssetAction> {
        let level = level.to_string();
        let edit = |edit: LevelEdit| Some(AssetAction::EditLevel(level.clone(), edit));
        let (base, base_path) = &self.stands_on;
        if let Some(source) = ui.dropped_onto(base.field) {
            return Some(AssetAction::DropStandsOn { level, source });
        }
        if ui.clicked(base.arrow) {
            return Some(AssetAction::BrowseStandsOn { level, anchor: base.field });
        }
        if let Some(path) = base_path.clone().filter(|_| ui.clicked(base.field)) {
            return Some(AssetAction::Ping(path));
        }
        if let Some(source) = ui.dropped_onto(self.add) {
            return Some(AssetAction::DropScene { level, at: None, source });
        }
        if ui.clicked(self.add) {
            return Some(AssetAction::BrowseScene { level, at: None, anchor: self.add });
        }
        if let Some((from, to)) = collection::moved(ui, self.items) {
            return edit(LevelEdit::Move { from, to });
        }
        for (at, scene) in self.scenes.iter().enumerate() {
            if let Some(source) = ui.dropped_onto(scene.field) {
                return Some(AssetAction::DropScene { level, at: Some(at), source });
            }
            // The arrow first, as a click on it is a click on the field too.
            if ui.clicked(scene.arrow) {
                return Some(AssetAction::BrowseScene { level, at: Some(at), anchor: scene.field });
            }
            if ui.clicked(scene.field) {
                return Some(AssetAction::Ping(scene.path.clone()));
            }
            if ui.changed(scene.active) {
                if ui.is_checked(scene.active) {
                    return edit(LevelEdit::Activate(at));
                }
                ui.set_checked(scene.active, true);
            }
            if ui.changed(scene.required) {
                return edit(LevelEdit::Require(at, ui.is_checked(scene.required)));
            }
            if ui.changed(scene.reload) {
                return edit(LevelEdit::Reload(at, ui.is_checked(scene.reload)));
            }
            if ui.clicked(scene.remove) {
                return edit(LevelEdit::Remove(at));
            }
            // A weight is written once typed: on Enter, or when the field is left.
            let (field, shown) = scene.weight;
            let typed = ui.text(field).trim().parse::<f32>().ok().filter(|w| w.is_finite() && *w >= 0.0);
            let done = ui.submitted(field) || ui.focused() != Some(field);
            if let Some(weight) = typed.filter(|&weight| done && weight != shown) {
                return edit(LevelEdit::Weight(at, weight));
            }
        }
        None
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
        ItemKind::Asset(AssetKind::Sound) => "Sound",
        ItemKind::Asset(AssetKind::InputActions) => "Input actions",
        ItemKind::Asset(AssetKind::Strings) => "String table",
        ItemKind::Asset(AssetKind::AssetTable) => "Asset table",
        ItemKind::Asset(AssetKind::Paths) => "Sound paths",
        ItemKind::Asset(AssetKind::LightProfile) => "IES light profile",
        ItemKind::Asset(AssetKind::Lighting) => "Lighting settings",
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
        ItemKind::Asset(AssetKind::Sound) => "audio",
        ItemKind::Asset(AssetKind::InputActions) => "gamepad",
        ItemKind::Asset(AssetKind::Strings | AssetKind::AssetTable) => "globe",
        ItemKind::Asset(AssetKind::Paths) => "branch",
        ItemKind::Asset(AssetKind::LightProfile) => "light",
        ItemKind::Asset(AssetKind::Lighting) => "light",
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
