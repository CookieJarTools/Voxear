//! The object picker, as Unity's: what a reference can point at, searched.
//! A click on a field that takes an Obj or an asset (a script's reference,
//! a Mesh Renderer's mesh or material, a level's scene) opens it under the
//! field: a title naming what the field takes, a search box to type in at
//! once, and None and everything the field can take, each with its icon
//! and where it is (an asset's folder, an Obj's scene). Typing narrows the list as the
//! command palette does, letters in order and the best first. Up and Down
//! move the highlight, which starts on what the field points at now; Enter
//! or a click picks, and Escape or a click elsewhere closes it. Materials,
//! prefabs, models and meshes show previews in place of their icons once
//! the editor has them drawn.
//!
//! It also lists what Add Component offers, in folders as Unity's does:
//! with nothing typed, the folder open shows its folders, then what is in
//! it, and a folder picked (or Right) opens it, with a back button (or
//! Left) to the one it is in. Typing searches everything in every folder,
//! each row saying which folder it is in.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_engine::{AssetPreviews, Game};
use voxear_ui::{ImageRef, ItemFns, NodeId, Style, Ui};

use crate::{
    fields::Pick,
    hierarchy,
    palette::{PaletteKey, score},
    props::{Part, Prop},
};

/// How tall a row is, as `item-height` gives it in Editor.vui, and the
/// tallest the list grows before it scrolls.
const ROW: f32 = 28.0;
const TALLEST: f32 = 308.0;
/// How wide the picker is at the least, and how far it keeps from the
/// window's edges.
const WIDTH: f32 = 340.0;
const MARGIN: f32 = 8.0;
/// How big a row's preview shows, as Editor.vss sizes `picker-thumb`.
pub const PREVIEW: f32 = 22.0;

/// One row of the picker.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    /// Where it is: an asset's folder, an Obj's scene.
    pub detail: String,
    pub icon: &'static str,
    pub pick: Chosen,
    /// What to show a preview of in place of the icon, when it has one: an
    /// asset's path, or a built-in mesh.
    pub preview: Option<String>,
    /// The folder it is listed in, as `Scripts/Enemies`, or empty for the
    /// top, where a field's choices all are.
    pub folder: String,
}

/// What picking a row does: points a reference at what it is, adds a
/// component, or opens the folder it names.
#[derive(Clone, Debug, PartialEq)]
pub enum Chosen {
    Value(Pick),
    Component(Part),
    Folder(String),
}

/// What a pick is for: a reference of an Obj's, or a scene of a level's.
#[derive(Clone, Debug, PartialEq)]
pub enum PickFor {
    /// The Obj and the value of it to point elsewhere.
    Field(Entity, Prop),
    /// The level, by its path, and the place of the scene to change, or
    /// none for a scene to add.
    LevelScene { level: String, at: Option<usize> },
    /// The Obj to add a component to.
    AddComponent(Entity),
    /// The level the level at this path stands on.
    StandsOn { level: String },
    /// A map of the material at this path, for a picture.
    MaterialMap { material: String, map: voxear_engine::MaterialMap },
    /// A material of the model at this path, for the `.material` it draws with.
    Remap { model: String, part: String },
}

/// What the picker is open for, and the field it opened from.
struct Opened {
    target: PickFor,
    anchor: NodeId,
}

pub struct ObjectPicker {
    panel: NodeId,
    back: NodeId,
    title: NodeId,
    field: NodeId,
    list: NodeId,
    empty: NodeId,
    every: Vec<Choice>,
    shown: Rc<RefCell<Vec<Choice>>>,
    /// Previews drawn so far, by what they show.
    previews: Rc<RefCell<HashMap<String, ImageRef>>>,
    current: usize,
    opened: Option<Opened>,
    /// What the picker was opened as, and the folder open, empty at the top.
    heading: String,
    folder: String,
    /// The search box can take focus only once it has been laid out, a
    /// frame or so after the picker shows; until it has, the picker stays
    /// open whatever has focus.
    focusing: bool,
}

impl ObjectPicker {
    pub fn new(ui: &mut Ui, panel: NodeId, (back, title): (NodeId, NodeId), field: NodeId, list: NodeId, empty: NodeId) -> Self {
        let shown: Rc<RefCell<Vec<Choice>>> = Rc::default();
        let previews: Rc<RefCell<HashMap<String, ImageRef>>> = Rc::default();
        let (bound, drawn) = (shown.clone(), previews.clone());
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let thumb = ui.panel(row, Style::default());
                ui.add_class(thumb, "picker-thumb");
                let icon = ui.icon(thumb, "obj");
                ui.add_class(icon, "palette-icon");
                let image = ui.image(thumb);
                ui.add_class(image, "thumb-image");
                let label = ui.add_label(row, "");
                ui.add_class(label, "palette-title");
                let detail = ui.add_label(row, "");
                ui.add_class(detail, "palette-detail");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let shown = bound.borrow();
                let Some(choice) = shown.get(index) else { return };
                let parts = ui.children(row);
                let thumb = ui.children(parts[0]);
                let preview = choice.preview.as_ref().and_then(|reference| drawn.borrow().get(reference).copied());
                ui.set_icon(thumb[0], choice.icon);
                ui.set_visible(thumb[0], preview.is_none());
                ui.set_image(thumb[1], preview);
                ui.set_visible(thumb[1], preview.is_some());
                ui.set_text(parts[1], &choice.label);
                ui.set_text(parts[2], &choice.detail);
            },
        };
        ui.bind_list(list, 0, items);
        ui.set_visible(panel, false);
        ui.set_visible(back, false);
        Self {
            panel,
            back,
            title,
            field,
            list,
            empty,
            every: Vec::new(),
            shown,
            previews,
            current: 0,
            opened: None,
            heading: String::new(),
            folder: String::new(),
            focusing: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.opened.is_some()
    }

    /// What the rows listed now can show previews of.
    pub fn previewable(&self) -> Vec<String> {
        self.shown.borrow().iter().filter_map(|choice| choice.preview.clone()).collect()
    }

    /// Shows these previews in place of icons, by what they show.
    pub fn set_previews(&mut self, ui: &mut Ui, previews: HashMap<String, ImageRef>) {
        if *self.previews.borrow() == previews {
            return;
        }
        *self.previews.borrow_mut() = previews;
        ui.refresh_list(self.list);
    }

    /// Opens under `anchor` for `target`, headed `title`, offering
    /// `choices`, at the top of their folders, with the one called `now`
    /// highlighted.
    pub fn open(
        &mut self,
        ui: &mut Ui,
        (target, anchor): (PickFor, NodeId),
        title: &str,
        choices: Vec<Choice>,
        now: &str,
    ) {
        self.every = choices;
        self.opened = Some(Opened { target, anchor });
        self.heading = title.to_string();
        ui.set_text(self.field, "");
        ui.set_visible(self.panel, true);
        self.open_folder(ui, String::new());
        let at = self.every.iter().position(|c| c.label == now || c.label.rsplit('/').next() == Some(now));
        self.highlight(ui, at.unwrap_or(0));
        self.place(ui);
        self.focusing = true;
    }

    /// Narrows as the search is typed in, and closes when focus goes
    /// elsewhere. Returns a click's pick, with what it is for.
    pub fn update(&mut self, ui: &mut Ui) -> Option<(PickFor, Chosen)> {
        self.opened.as_ref()?;
        if ui.clicked(self.back) {
            self.up(ui);
        }
        if ui.clicked(self.list) {
            self.current = ui.selection(self.list).first().copied().unwrap_or(self.current);
            return self.pick(ui);
        }
        if self.focusing {
            ui.focus(self.field);
            self.focusing = ui.focused() != Some(self.field);
        } else if ui.is_pressed(self.panel) {
            // A press inside it, as on a row, takes focus from the search while the button is down, which
            // spans frames: the search has it back, so the picker stays open for the click to pick.
            ui.focus(self.field);
        } else if ui.focused() != Some(self.field) {
            self.close(ui);
            return None;
        }
        if ui.changed(self.field) {
            self.filter(ui);
            self.highlight(ui, 0);
        }
        self.place(ui);
        None
    }

    /// Left goes back to the folder the open one is in, and Right opens the
    /// folder highlighted, while nothing is typed. Returns whether it did.
    pub fn side_key(&mut self, ui: &mut Ui, right: bool) -> bool {
        if self.opened.is_none() || !ui.text(self.field).is_empty() {
            return false;
        }
        if !right {
            return self.up(ui);
        }
        let folder = match self.shown.borrow().get(self.current) {
            Some(Choice { pick: Chosen::Folder(folder), .. }) => folder.clone(),
            _ => return false,
        };
        self.open_folder(ui, folder);
        true
    }

    /// Up and Down move the highlight; Enter picks it; Escape closes.
    pub fn key(&mut self, ui: &mut Ui, key: PaletteKey) -> Option<(PickFor, Chosen)> {
        let count = self.shown.borrow().len();
        match key {
            PaletteKey::Up => self.highlight(ui, self.current.saturating_sub(1)),
            PaletteKey::Down => self.highlight(ui, (self.current + 1).min(count.saturating_sub(1))),
            PaletteKey::Enter => return self.pick(ui),
            PaletteKey::Escape => self.close(ui),
        }
        None
    }

    /// Picks the row highlighted, or opens it when it is a folder.
    fn pick(&mut self, ui: &mut Ui) -> Option<(PickFor, Chosen)> {
        let choice = self.shown.borrow().get(self.current).map(|c| c.pick.clone());
        if let Some(Chosen::Folder(folder)) = choice {
            ui.set_text(self.field, "");
            self.open_folder(ui, folder);
            return None;
        }
        let opened = self.opened.take();
        self.close(ui);
        Some((opened?.target, choice?))
    }

    /// Goes back to the folder the open one is in. False at the top.
    fn up(&mut self, ui: &mut Ui) -> bool {
        if self.folder.is_empty() {
            return false;
        }
        let from = std::mem::take(&mut self.folder);
        let parent = from.rsplit_once('/').map_or(String::new(), |(parent, _)| parent.to_string());
        self.open_folder(ui, parent);
        // On the folder it came out of, as a file browser does.
        let at = self.shown.borrow().iter().position(|c| c.pick == Chosen::Folder(from.clone()));
        self.highlight(ui, at.unwrap_or(0));
        true
    }

    /// Lists a folder, its name the title and the back button showing
    /// inside one. The search keeps focus, which a click on a folder or the
    /// back button took, so the picker stays open.
    fn open_folder(&mut self, ui: &mut Ui, folder: String) {
        if !self.focusing {
            ui.focus(self.field);
        }
        let name = folder.rsplit('/').next().unwrap_or_default().to_string();
        ui.set_text(self.title, if folder.is_empty() { &self.heading } else { &name });
        ui.set_visible(self.back, !folder.is_empty());
        self.folder = folder;
        self.filter(ui);
        self.highlight(ui, 0);
    }

    fn close(&mut self, ui: &mut Ui) {
        self.opened = None;
        self.focusing = false;
        ui.set_visible(self.panel, false);
        if ui.focused() == Some(self.field) {
            ui.blur();
        }
    }

    /// What is typed against every choice in every folder, best first;
    /// with nothing typed, the open folder's folders, then what is in it.
    fn filter(&mut self, ui: &mut Ui) {
        let query = ui.text(self.field);
        let query = query.trim();
        let shown: Vec<Choice> = if query.is_empty() {
            let mut shown = subfolders(&self.every, &self.folder);
            shown.extend(self.every.iter().filter(|c| c.folder == self.folder).cloned());
            shown
        } else {
            let mut ranked: Vec<(i32, usize)> = self
                .every
                .iter()
                .enumerate()
                .filter_map(|(at, c)| {
                    let by_name = score(query, &c.label);
                    let place = format!("{} {} {}", c.folder.replace('/', " "), c.detail, c.label);
                    let by_place = score(query, &place).map(|s| s - 64 * 20);
                    Some((by_name.max(by_place)?, at))
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            // Found in a folder, a row says which.
            let placed = |mut choice: Choice| {
                if choice.detail.is_empty() {
                    choice.detail.clone_from(&choice.folder);
                }
                choice
            };
            ranked.into_iter().map(|(_, at)| placed(self.every[at].clone())).collect()
        };
        let count = shown.len();
        *self.shown.borrow_mut() = shown;
        ui.set_list_count(self.list, count);
        ui.refresh_list(self.list);
        ui.set_visible(self.empty, count == 0);
        ui.set_inline_style(self.list, &format!("height: {}px", (count as f32 * ROW).min(TALLEST)));
    }

    fn highlight(&mut self, ui: &mut Ui, index: usize) {
        self.current = index;
        if self.shown.borrow().is_empty() {
            ui.set_selection(self.list, &[]);
            return;
        }
        ui.set_selection(self.list, &[index]);
        ui.scroll_to_index(self.list, index);
    }

    /// Under the field it opened from, as wide as it or wider, kept inside
    /// the window: above the field when there is no room below.
    fn place(&self, ui: &mut Ui) {
        let Some(opened) = &self.opened else { return };
        // Over the window its field is in, a desktop window of a panel floated out too.
        ui.show_over_window(self.panel, ui.window_of(opened.anchor));
        let [x, y, width, height] = ui.rect(opened.anchor);
        let (window_width, window_height) = ui.screen_size(opened.anchor);
        let [_, _, _, own_height] = ui.rect(self.panel);
        let wide = width.max(WIDTH);
        let left = x.min(window_width - wide - MARGIN).max(MARGIN);
        let below = y + height + 4.0;
        let top = if below + own_height > window_height - MARGIN { (y - own_height - 4.0).max(MARGIN) } else { below };
        ui.set_inline_style(self.panel, &format!("left: {left}px; top: {top}px; width: {wide}px"));
    }
}

/// The folders directly inside `folder` that hold any choice, in the
/// order their first choice comes.
fn subfolders(every: &[Choice], folder: &str) -> Vec<Choice> {
    let mut found: Vec<Choice> = Vec::new();
    for choice in every {
        let inside = match folder {
            "" => Some(choice.folder.as_str()),
            _ => choice.folder.strip_prefix(folder).and_then(|rest| rest.strip_prefix('/')),
        };
        let Some(next) = inside.and_then(|rest| rest.split('/').next()).filter(|next| !next.is_empty()) else {
            continue;
        };
        let path = if folder.is_empty() { next.to_string() } else { format!("{folder}/{next}") };
        if !found.iter().any(|f| f.pick == Chosen::Folder(path.clone())) {
            let pick = Chosen::Folder(path);
            let (label, detail) = (next.to_string(), "›".to_string());
            found.push(Choice { label, detail, icon: "folder", pick, preview: None, folder: folder.into() });
        }
    }
    found
}

/// The choices as the picker lists them: an Obj with the icon the
/// Hierarchy gives it and its scene, an asset with its kind's icon and its
/// folder, and a built-in mesh as built in.
pub fn decorate(game: &Game, choices: Vec<(String, Pick)>) -> Vec<Choice> {
    choices
        .into_iter()
        .map(|(label, pick)| {
            let (icon, detail) = match &pick {
                Pick::Obj(None) | Pick::Asset(None) if label == "None" => ("close", String::new()),
                Pick::Obj(None) | Pick::Asset(None) => ("material", "Built in".into()),
                Pick::Obj(Some(entity)) => obj(game, *entity),
                Pick::Asset(Some(reference)) if reference.starts_with("builtin:") => ("cube", "Built in".into()),
                // A part of a model, as a mesh, beside the model it is in.
                Pick::Asset(Some(reference)) if reference.contains('#') => {
                    let model = reference.split_once('#').map_or("", |(model, _)| model);
                    ("box", file_name(model).to_string())
                }
                Pick::Asset(Some(path)) => asset(game, path),
            };
            let label = match &pick {
                Pick::Asset(Some(path)) if !path.starts_with("builtin:") => file_name(&label).to_string(),
                _ => label,
            };
            let preview = match &pick {
                Pick::Asset(Some(reference)) if reference.starts_with("builtin:") => Some(reference.clone()),
                Pick::Asset(Some(path)) => {
                    let db = game.content.db();
                    let kind = db.guid_of(path).and_then(|guid| db.get(guid)).map(|e| e.kind);
                    kind.filter(|&kind| AssetPreviews::shows(kind)).map(|_| path.clone())
                }
                _ => None,
            };
            Choice { label, detail, icon, pick: Chosen::Value(pick), preview, folder: String::new() }
        })
        .collect()
}

fn obj(game: &Game, entity: Entity) -> (&'static str, String) {
    let world = &game.engine.world;
    let icon = hierarchy::icon(world, entity);
    let scene = game.levels.scene_of(entity).and_then(|guid| game.content.db().get(guid)).map(|e| e.path.clone());
    (icon, scene.as_deref().map(file_name).unwrap_or_default().to_string())
}

fn asset(game: &Game, path: &str) -> (&'static str, String) {
    let db = game.content.db();
    let kind = db.guid_of(path).and_then(|guid| db.get(guid)).map(|e| e.kind);
    let icon = match kind {
        Some(AssetKind::Level) => "level",
        Some(AssetKind::Scene) => "scene",
        Some(AssetKind::Prefab) => "cube",
        Some(AssetKind::Model) => "box",
        Some(AssetKind::Material) => "material",
        Some(AssetKind::Texture) => "texture",
        Some(AssetKind::Ui) => "panels",
        Some(AssetKind::Sound) => "audio",
        None => "obj",
    };
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    (icon, folder.to_string())
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
