//! The command palette (Ctrl+K), as in Linear and Unity's search: a list
//! under the command bar of everything there is to do or go to, narrowed as
//! a few letters are typed. It holds the editor's commands with their
//! shortcuts, commands added by code (see [`PaletteCommand`] and the
//! scripts' `[Command]` methods), the settings, every Obj, the levels to
//! open, the project's assets to show in the Project, and the VoxDocs
//! pages.
//!
//! Matching is fuzzy: the letters typed need only come in order, and a
//! result ranks higher the more of them start words or follow one another,
//! so "mt" finds "Move tool" before "Material". What was run recently is
//! remembered per project (by each result's key, see `settings::Memory`):
//! with nothing typed it comes first, and it ranks higher when typed for.
//! Up and Down move the highlight, Enter or a click runs it, and Escape
//! closes the palette.

use std::{cell::RefCell, rc::Rc};

use voxear_assets::AssetKind;
use voxear_ecs::Entity;
use voxear_engine::{Game, modules::Modules};
use voxear_scene::{Destroying, EditorOnly, Name};
use voxear_ui::{ItemFns, NodeId, Ui};

use crate::{
    editor::{Command, Editor},
    objs::NEW_OBJS,
    settings::{SETTINGS, Settings},
};

/// The most results shown at once; more narrow down as letters are typed.
const MOST: usize = 200;
/// How tall a result is, as `item-height` gives it in Editor.vui, and the
/// tallest the list grows before it scrolls.
const ROW: f32 = 28.0;
const TALLEST: f32 = 336.0;
/// How much more a recent result counts when typed for, against one match.
const RECENT_BONUS: i32 = 30 * 64;
/// The docked panels, by id, as the palette offers to show them.
const PANELS: [(&str, &str); 14] = [
    ("Hierarchy", "hierarchy"),
    ("Levels", "levels"),
    ("Scene", "scene"),
    ("Game", "game"),
    ("VoxDocs", "docs"),
    ("Project", "project"),
    ("Console", "console"),
    ("Input Actions", "input"),
    ("Audio", "audio"),
    ("Localization Tables", "localization"),
    ("Package Manager", "packages"),
    ("Generator", "generator"),
    ("Inspector", "inspector"),
    ("Settings", "settings"),
];

/// A command for the palette from code built on the editor: what it is
/// called, keys that run it from anywhere ("Ctrl+Shift+B", or empty), its
/// icon, and what it does. `id` names it, to remember it among the recent.
#[derive(Clone)]
pub struct PaletteCommand {
    pub id: String,
    pub title: String,
    pub keys: String,
    pub icon: &'static str,
    pub run: Rc<dyn Fn(&mut Editor)>,
}

/// What a result does when it is run.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteAction {
    Command(Command),
    /// Play, or stop playing.
    Play,
    Pause,
    Step,
    /// The Scene view's tool, by its place on the tool strip.
    Tool(usize),
    /// Make one of `NEW_OBJS` at the top of the Hierarchy.
    Create(usize),
    /// Make the Objs of a prefab the scripts add to Create Object, by its
    /// place among their `[CreateObject]`s.
    CreateFrom(usize),
    /// Bring a docked panel to the front, by its id.
    ShowPanel(&'static str),
    Select(Entity),
    OpenLevel(String),
    /// Show an asset or script in the Project, by its path.
    Reveal(String),
    /// Open a VoxDocs page, by its reference.
    OpenDoc(String),
    /// A setting, by its key: a toggle flips, a choice moves to the next,
    /// and a number opens the Settings panel to type it.
    Setting(&'static str),
    /// A [`PaletteCommand`], by its id.
    Added(String),
    /// A scripts' `[Command]` method: its type and its name.
    Script {
        type_name: String,
        method: String,
    },
}

/// A result run: what it does, and its key, to remember it by.
#[derive(Clone, Debug, PartialEq)]
pub struct Picked {
    pub action: PaletteAction,
    pub key: String,
}

/// A command from code, as the palette lists it.
pub struct Added {
    pub key: String,
    pub title: String,
    pub keys: String,
    pub icon: &'static str,
    pub action: PaletteAction,
}

/// What the palette offers, gathered when it opens.
pub struct Sources<'a> {
    pub game: &'a Game,
    pub playing: bool,
    pub settings: &'a Settings,
    /// Keys of what was run recently, most recent first.
    pub recent: &'a [String],
    /// The VoxDocs pages, as titles and references.
    pub docs: Vec<(String, String)>,
    pub added: Vec<Added>,
}

/// One result: what it says, what kind of thing it is or its shortcut, its
/// icon, what it does, the key it is remembered by, and whether it is a
/// command, which shows before anything is typed.
#[derive(Clone, Debug)]
struct Entry {
    title: String,
    detail: String,
    icon: &'static str,
    action: PaletteAction,
    key: String,
    command: bool,
}

pub struct Palette {
    field: NodeId,
    panel: NodeId,
    list: NodeId,
    empty: NodeId,
    /// Everything there is and the keys run recently, gathered when the
    /// palette opens, and what the letters typed leave of it, best first.
    every: Vec<Entry>,
    recent: Vec<String>,
    shown: Rc<RefCell<Vec<Entry>>>,
    current: usize,
    open: bool,
}

impl Palette {
    pub fn new(ui: &mut Ui, field: NodeId, panel: NodeId, list: NodeId, empty: NodeId) -> Self {
        let shown: Rc<RefCell<Vec<Entry>>> = Rc::default();
        let bound = shown.clone();
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let icon = ui.icon(row, "search");
                ui.add_class(icon, "palette-icon");
                let title = ui.add_label(row, "");
                ui.add_class(title, "palette-title");
                let detail = ui.add_label(row, "");
                ui.add_class(detail, "palette-detail");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let shown = bound.borrow();
                let Some(entry) = shown.get(index) else { return };
                let parts = ui.children(row);
                ui.set_icon(parts[0], entry.icon);
                ui.set_text(parts[1], &entry.title);
                ui.set_text(parts[2], &entry.detail);
            },
        };
        ui.bind_list(list, 0, items);
        ui.set_visible(panel, false);
        Self { field, panel, list, empty, every: Vec::new(), recent: Vec::new(), shown, current: 0, open: false }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens when the command bar gets focus, gathering what it offers from
    /// `sources`; narrows as it is typed in; and closes when focus goes
    /// elsewhere. Returns what a click on a result asks to run.
    pub fn update<'a>(&mut self, ui: &mut Ui, sources: impl FnOnce() -> Sources<'a>) -> Option<Picked> {
        let focused = ui.focused() == Some(self.field);
        if self.open && ui.clicked(self.list) {
            self.current = ui.selection(self.list).first().copied().unwrap_or(self.current);
            return self.run(ui);
        }
        if focused && !self.open {
            let sources = sources();
            self.recent = sources.recent.to_vec();
            self.every = gather(&sources);
            self.open = true;
            self.filter(ui);
            ui.set_visible(self.panel, true);
        } else if self.open && !focused {
            self.close(ui);
        } else if self.open && ui.changed(self.field) {
            self.filter(ui);
        }
        if self.open {
            self.place(ui);
        }
        None
    }

    /// Up and Down move the highlight; Enter runs it; Escape closes.
    pub fn key(&mut self, ui: &mut Ui, key: PaletteKey) -> Option<Picked> {
        let count = self.shown.borrow().len();
        match key {
            PaletteKey::Up => self.highlight(ui, self.current.saturating_sub(1)),
            PaletteKey::Down => self.highlight(ui, (self.current + 1).min(count.saturating_sub(1))),
            PaletteKey::Enter => return self.run(ui),
            PaletteKey::Escape => {
                ui.set_text(self.field, "");
                ui.blur();
                self.close(ui);
            }
        }
        None
    }

    /// Runs the highlighted result, and closes, leaving the bar empty.
    fn run(&mut self, ui: &mut Ui) -> Option<Picked> {
        let shown = self.shown.borrow();
        let picked = shown.get(self.current).map(|e| Picked { action: e.action.clone(), key: e.key.clone() });
        drop(shown);
        ui.set_text(self.field, "");
        ui.blur();
        self.close(ui);
        picked
    }

    fn close(&mut self, ui: &mut Ui) {
        self.open = false;
        ui.set_visible(self.panel, false);
    }

    /// Ranks everything against what is typed and shows the best.
    fn filter(&mut self, ui: &mut Ui) {
        let query = ui.text(self.field);
        let shown = if query.trim().is_empty() { self.untyped() } else { self.ranked(query.trim()) };
        let count = shown.len();
        *self.shown.borrow_mut() = shown;
        ui.set_list_count(self.list, count);
        ui.refresh_list(self.list);
        ui.set_visible(self.empty, count == 0);
        ui.set_inline_style(self.list, &format!("height: {}px", (count as f32 * ROW).min(TALLEST)));
        self.highlight(ui, 0);
    }

    /// With nothing typed: what was run recently, then the commands, in
    /// their order.
    fn untyped(&self) -> Vec<Entry> {
        let mut recent: Vec<(usize, Entry)> =
            self.every.iter().filter_map(|e| Some((self.recency(e)?, e.clone()))).collect();
        recent.sort_by_key(|(at, _)| *at);
        let recent = recent.into_iter().map(|(_, mut entry)| {
            entry.detail =
                if entry.detail.is_empty() { "Recent".into() } else { format!("Recent · {}", entry.detail) };
            entry
        });
        let commands = self.every.iter().filter(|e| e.command && self.recency(e).is_none()).cloned();
        recent.chain(commands).take(MOST).collect()
    }

    /// What matches what is typed, best first; equal scores keep the order
    /// they were gathered in, commands first.
    fn ranked(&self, query: &str) -> Vec<Entry> {
        let mut ranked: Vec<(i32, usize)> = self
            .every
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                let bonus = if self.recency(entry).is_some() { RECENT_BONUS } else { 0 };
                Some((score(query, &entry.title)? + bonus, i))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        ranked.into_iter().take(MOST).map(|(_, i)| self.every[i].clone()).collect()
    }

    /// How recently a result was run: 0 for the last, or `None`.
    fn recency(&self, entry: &Entry) -> Option<usize> {
        self.recent.iter().position(|key| *key == entry.key)
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

    /// Under the command bar.
    fn place(&self, ui: &mut Ui) {
        let Some(bar) = ui.parent(self.field) else { return };
        let [x, y, _, height] = ui.rect(bar);
        ui.set_inline_style(self.panel, &format!("left: {x}px; top: {}px", y + height + 6.0));
    }
}

/// The keys the palette takes while it is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteKey {
    Up,
    Down,
    Enter,
    Escape,
}

/// The command a title names, as the palette shows it, ignoring case: the
/// editor's own and those the scripts add, as a VoxDocs guide's step runs one.
pub fn command_named(sources: &Sources, title: &str) -> Option<PaletteAction> {
    let title = title.trim();
    gather(sources).into_iter().find(|entry| entry.command && entry.title.eq_ignore_ascii_case(title)).map(|e| e.action)
}

/// Every command's title, as the palette lists them: the editor's own, then
/// those the scripts add, for a VoxDocs page's Action… menu.
pub fn command_titles(sources: &Sources) -> Vec<String> {
    gather(sources).into_iter().filter(|entry| entry.command).map(|entry| entry.title).collect()
}

/// Everything the palette offers: the commands first, those added by code,
/// the settings, then Objs, levels, assets, and the VoxDocs pages.
fn gather(sources: &Sources) -> Vec<Entry> {
    let mut every = commands(sources.playing, &sources.game.project.modules());
    every.extend(sources.added.iter().map(|added| Entry {
        title: added.title.clone(),
        detail: added.keys.clone(),
        icon: added.icon,
        action: added.action.clone(),
        key: added.key.clone(),
        command: true,
    }));
    every.extend(SETTINGS.iter().map(|info| Entry {
        title: info.title.to_string(),
        detail: format!("Setting · {}", sources.settings.describe(info)),
        icon: "settings",
        action: PaletteAction::Setting(info.key),
        key: format!("setting:{}", info.key),
        command: false,
    }));
    every.extend(objs(sources.game));
    every.extend(assets(sources.game));
    every.extend(sources.docs.iter().map(|(title, reference)| Entry {
        title: title.clone(),
        detail: doc_detail(reference).into(),
        icon: "book",
        action: PaletteAction::OpenDoc(reference.clone()),
        key: format!("doc:{reference}"),
        command: false,
    }));
    every
}

/// Where a VoxDocs page is from, beside its title.
fn doc_detail(reference: &str) -> &'static str {
    match reference {
        reference if reference.starts_with(voxear_docs::MANUAL_SCHEME) => "Manual",
        reference if reference.starts_with(voxear_docs::PACKAGE_SCHEME) => "Package",
        _ => "Doc",
    }
}

/// Every Obj, by name. Each is remembered by its scene and its place there,
/// which stay the same from run to run, or by its name when no scene keeps
/// it.
fn objs(game: &Game) -> Vec<Entry> {
    let world = &game.engine.world;
    let mut objs: Vec<Entry> = world
        .query_ref::<(Entity, &Name)>()
        .without::<EditorOnly>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, name)| {
            let key = match game.levels.tracked(&[entity]).first() {
                Some(tracked) => format!("obj:{}:{}", tracked.scene, tracked.path),
                None => format!("obj-named:{}", name.0),
            };
            Entry {
                title: name.0.clone(),
                detail: "Obj".into(),
                icon: crate::hierarchy::icon(world, entity),
                action: PaletteAction::Select(entity),
                key,
                command: false,
            }
        })
        .collect();
    objs.sort_by(|a, b| a.title.cmp(&b.title));
    objs
}

/// The project's assets: levels to open, and the rest to show in the
/// Project.
fn assets(game: &Game) -> Vec<Entry> {
    let mut assets: Vec<(String, AssetKind)> = game.content.db().entries().map(|e| (e.path.clone(), e.kind)).collect();
    assets.sort_by(|a, b| a.0.cmp(&b.0));
    assets
        .into_iter()
        .map(|(path, kind)| {
            let (detail, icon) = match kind {
                AssetKind::Level => ("Level", "level"),
                AssetKind::Scene => ("Scene", "scene"),
                AssetKind::Prefab => ("Prefab", "cube"),
                AssetKind::Model => ("Model", "box"),
                AssetKind::Material => ("Material", "material"),
                AssetKind::Texture => ("Texture", "texture"),
                AssetKind::Ui => ("UI document", "panels"),
                AssetKind::Sound => ("Sound", "audio"),
                AssetKind::InputActions => ("Input actions", "gamepad"),
                AssetKind::Strings => ("String table", "globe"),
                AssetKind::AssetTable => ("Asset table", "globe"),
                AssetKind::Paths => ("Sound paths", "branch"),
                AssetKind::LightProfile => ("IES light profile", "light"),
                AssetKind::Shader => ("Shader", "shader"),
                AssetKind::MotionSet => ("Motion Set", "play"),
                AssetKind::GunHandling => ("Gun Handling", "crosshair"),
                AssetKind::Particles => ("Particle Effect", "particles"),
                AssetKind::PromptIcons => ("Prompt Icons", "gamepad"),
                AssetKind::Terrain => ("Terrain", "mountain"),
                AssetKind::WorldGen => ("World Generator", "sprout"),
                AssetKind::BlockWorld => ("Block World", "blocks"),
                AssetKind::Block => ("Block", "block"),
                AssetKind::VoxModel => ("Block Model", "blocks"),
                AssetKind::Lighting => ("Lighting settings", "light"),
                AssetKind::Data => ("Data asset", "file"),
            };
            let (action, key) = match kind {
                AssetKind::Level => (PaletteAction::OpenLevel(path.clone()), format!("level:{path}")),
                _ => (PaletteAction::Reveal(path.clone()), format!("asset:{path}")),
            };
            let title = path.rsplit('/').next().unwrap_or(&path).to_string();
            Entry { title, detail: detail.into(), icon, action, key, command: false }
        })
        .collect()
}

/// The editor's commands, with the keys that do the same.
fn commands(playing: bool, modules: &Modules) -> Vec<Entry> {
    let command = |title: &str, keys: &str, icon: &'static str, action: PaletteAction, key: String| Entry {
        title: title.to_string(),
        detail: keys.to_string(),
        icon,
        action,
        key,
        command: true,
    };
    let mut commands = vec![
        command(if playing { "Stop" } else { "Play" }, "", "play", PaletteAction::Play, "play".into()),
        command("Pause", "", "pause", PaletteAction::Pause, "pause".into()),
        command("Step", "", "step", PaletteAction::Step, "step".into()),
    ];
    let editing = [
        ("Save", "Ctrl+S", "file", Command::Save),
        ("Undo", "Ctrl+Z", "chevron-left", Command::Undo),
        ("Redo", "Ctrl+Y", "chevron-right", Command::Redo),
        ("Duplicate", "Ctrl+D", "copy", Command::Duplicate),
        ("Delete", "Delete", "close", Command::Delete),
        ("Frame Selected", "F", "maximize", Command::Frame),
        ("Align With View", "Ctrl+Shift+F", "camera", Command::AlignWithView),
        ("Edit Layers", "", "layers", Command::Layers),
        ("Pseudo-localise Text", "", "globe", Command::PseudoText),
        ("Bake Sound Paths", "", "branch", Command::BakePaths),
    ];
    for (title, keys, icon, run) in editing {
        commands.push(command(title, keys, icon, PaletteAction::Command(run), format!("command:{title}")));
    }
    let tools = [("Select", "Q", "select"), ("Move", "W", "move"), ("Rotate", "E", "rotate"), ("Scale", "R", "scale")];
    for (index, (name, keys, icon)) in tools.into_iter().enumerate() {
        commands.push(command(&format!("{name} tool"), keys, icon, PaletteAction::Tool(index), format!("tool:{name}")));
    }
    for (kind, (label, .., module)) in NEW_OBJS.iter().enumerate() {
        if module.is_some_and(|module| !modules.has(module)) {
            continue;
        }
        commands.push(command(
            &format!("New {label}"),
            "",
            "plus",
            PaletteAction::Create(kind),
            format!("new:{label}"),
        ));
    }
    for (title, id) in PANELS {
        commands.push(command(
            &format!("Show {title}"),
            "",
            "maximize",
            PaletteAction::ShowPanel(id),
            format!("panel:{id}"),
        ));
    }
    commands
}

/// How well `query` matches `text`, or `None` when its letters are not all
/// there in order. Letters that start words, or follow the last one
/// matched, count for more; letters skipped count against; and between
/// matches as good, a shorter text is nearer what was meant. The first
/// letter is tried wherever it is, so "tool" finds the "tool" of "Rotate
/// tool" rather than its first t.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.chars().collect();
    let query: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).map(lower).collect();
    let best = match query.first() {
        Some(&first) => {
            let starts = (0..text.len()).filter(|&i| lower(text[i]) == first);
            starts.filter_map(|start| matched(&query, &text, start)).max()?
        }
        None => 0,
    };
    Some(best * 64 - text.len().min(63) as i32)
}

/// How well `query` matches `text` with its first letter at `start` and
/// each one after at the next place it is.
fn matched(query: &[char], text: &[char], start: usize) -> Option<i32> {
    let mut total = 0;
    let mut at = 0;
    let mut last: Option<usize> = None;
    for (index, &wanted) in query.iter().enumerate() {
        let found = if index == 0 { start } else { (at..text.len()).find(|&i| lower(text[i]) == wanted)? };
        let starts_word = found == 0
            || matches!(text[found - 1], ' ' | '_' | '-' | '.' | '/')
            || (text[found].is_uppercase() && text[found - 1].is_lowercase());
        total += 10;
        if starts_word {
            total += 15;
        }
        if last.is_some_and(|l| l + 1 == found) {
            total += 8;
        }
        total -= (found - at) as i32;
        last = Some(found);
        at = found + 1;
    }
    Some(total)
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Keys that run a command, as written for one: "Ctrl+Shift+B", "F5".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keys {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    /// The key itself, lowercase: "b", "f5", "delete".
    pub key: String,
}

impl Keys {
    /// Reads keys as written for a command, or `None` for none.
    pub fn parse(text: &str) -> Option<Self> {
        let mut keys = Keys { control: false, shift: false, alt: false, key: String::new() };
        for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" | "cmd" => keys.control = true,
                "shift" => keys.shift = true,
                "alt" | "option" => keys.alt = true,
                key => keys.key = key.to_string(),
            }
        }
        (!keys.key.is_empty()).then_some(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_in_order_match_and_word_starts_rank_first() {
        assert!(score("spa", "Spinner A").is_some());
        assert!(score("aps", "Spinner A").is_none(), "the letters must come in order");
        assert!(score("spin", "Spinner") > score("spin", "Stop playing in"), "letters together count for more");
        assert!(score("mt", "Move tool") > score("mt", "Material"), "word starts count for more");
        assert!(score("save", "Save") > score("save", "Save As"), "and shorter is nearer");
        assert!(score("tool", "Rotate tool") > score("tool", "The Console"), "the word, not its first t");
        assert!(score("", "Anything").is_some(), "nothing typed matches everything");
    }

    #[test]
    fn keys_read_as_commands_write_them() {
        let keys = Keys::parse("Ctrl+Shift+B").unwrap();
        assert!(keys.control && keys.shift && !keys.alt && keys.key == "b");
        assert_eq!(Keys::parse("F5").unwrap().key, "f5");
        assert_eq!(Keys::parse(""), None);
    }
}
