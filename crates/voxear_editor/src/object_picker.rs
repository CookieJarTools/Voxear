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

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_engine::{AssetPreviews, Game};
use voxear_render::{Camera, Light, MeshRenderer};
use voxear_ui::{ImageRef, ItemFns, NodeId, Style, Ui};

use crate::{fields::Pick, hierarchy, palette::PaletteKey, palette::score, props::Prop};

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

/// One thing a field can point at.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    /// Where it is: an asset's folder, an Obj's scene.
    pub detail: String,
    pub icon: &'static str,
    pub pick: Pick,
    /// What to show a preview of in place of the icon, when it has one: an
    /// asset's path, or a built-in mesh.
    pub preview: Option<String>,
}

/// What a pick is for: a reference of an Obj's, or a scene of a level's.
#[derive(Clone, Debug, PartialEq)]
pub enum PickFor {
    /// The Obj and the value of it to point elsewhere.
    Field(Entity, Prop),
    /// The level, by its path, and the place of the scene to change, or
    /// none for a scene to add.
    LevelScene { level: String, at: Option<usize> },
}

/// What the picker is open for, and the field it opened from.
struct Opened {
    target: PickFor,
    anchor: NodeId,
}

pub struct ObjectPicker {
    panel: NodeId,
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
    /// The search box can take focus only once it has been laid out, a
    /// frame or so after the picker shows; until it has, the picker stays
    /// open whatever has focus.
    focusing: bool,
}

impl ObjectPicker {
    pub fn new(ui: &mut Ui, panel: NodeId, title: NodeId, field: NodeId, list: NodeId, empty: NodeId) -> Self {
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
        Self {
            panel,
            title,
            field,
            list,
            empty,
            every: Vec::new(),
            shown,
            previews,
            current: 0,
            opened: None,
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

    /// Opens under `anchor` for `target`, offering `choices`, with the one
    /// called `now` highlighted.
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
        ui.set_text(self.title, &format!("Select {title}"));
        ui.set_text(self.field, "");
        ui.set_visible(self.panel, true);
        self.filter(ui);
        let at = self.every.iter().position(|c| c.label == now || c.label.rsplit('/').next() == Some(now));
        self.highlight(ui, at.unwrap_or(0));
        self.place(ui);
        self.focusing = true;
    }

    /// Narrows as the search is typed in, and closes when focus goes
    /// elsewhere. Returns a click's pick, with what it is for.
    pub fn update(&mut self, ui: &mut Ui) -> Option<(PickFor, Pick)> {
        self.opened.as_ref()?;
        if ui.clicked(self.list) {
            self.current = ui.selection(self.list).first().copied().unwrap_or(self.current);
            return self.pick(ui);
        }
        if self.focusing {
            ui.focus(self.field);
            self.focusing = ui.focused() != Some(self.field);
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

    /// Up and Down move the highlight; Enter picks it; Escape closes.
    pub fn key(&mut self, ui: &mut Ui, key: PaletteKey) -> Option<(PickFor, Pick)> {
        let count = self.shown.borrow().len();
        match key {
            PaletteKey::Up => self.highlight(ui, self.current.saturating_sub(1)),
            PaletteKey::Down => self.highlight(ui, (self.current + 1).min(count.saturating_sub(1))),
            PaletteKey::Enter => return self.pick(ui),
            PaletteKey::Escape => self.close(ui),
        }
        None
    }

    fn pick(&mut self, ui: &mut Ui) -> Option<(PickFor, Pick)> {
        let choice = self.shown.borrow().get(self.current).map(|c| c.pick.clone());
        let opened = self.opened.take();
        self.close(ui);
        Some((opened?.target, choice?))
    }

    fn close(&mut self, ui: &mut Ui) {
        self.opened = None;
        self.focusing = false;
        ui.set_visible(self.panel, false);
        if ui.focused() == Some(self.field) {
            ui.blur();
        }
    }

    /// What is typed against every choice, best first; with nothing typed,
    /// all of them in their order.
    fn filter(&mut self, ui: &mut Ui) {
        let query = ui.text(self.field);
        let query = query.trim();
        let shown: Vec<Choice> = if query.is_empty() {
            self.every.clone()
        } else {
            let mut ranked: Vec<(i32, usize)> = self
                .every
                .iter()
                .enumerate()
                .filter_map(|(at, c)| {
                    let by_name = score(query, &c.label);
                    let by_place = score(query, &format!("{} {}", c.detail, c.label)).map(|s| s - 64 * 20);
                    Some((by_name.max(by_place)?, at))
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            ranked.into_iter().map(|(_, at)| self.every[at].clone()).collect()
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
        let [x, y, width, height] = ui.rect(opened.anchor);
        let [_, _, window_width, window_height] = ui.rect(ui.root());
        let [_, _, _, own_height] = ui.rect(self.panel);
        let wide = width.max(WIDTH);
        let left = x.min(window_width - wide - MARGIN).max(MARGIN);
        let below = y + height + 4.0;
        let top = if below + own_height > window_height - MARGIN { (y - own_height - 4.0).max(MARGIN) } else { below };
        ui.set_inline_style(self.panel, &format!("left: {left}px; top: {top}px; width: {wide}px"));
    }
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
            Choice { label, detail, icon, pick, preview }
        })
        .collect()
}

fn obj(game: &Game, entity: Entity) -> (&'static str, String) {
    let world = &game.engine.world;
    let icon =
        hierarchy::icon(world.has::<Camera>(entity), world.has::<Light>(entity), world.has::<MeshRenderer>(entity));
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
        None => "obj",
    };
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    (icon, folder.to_string())
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
