//! The Levels panel, as CookieJarTools' LayerCake window: every level, and
//! every scene that is in no level, to find by name. Stopped, clicking a
//! level opens it to edit, and its play button opens it and plays. Playing,
//! clicking a level changes to it as the game would, through the scripts'
//! `Level.LoadAsync`, so every event fires; the bar at the bottom follows any
//! level change, whoever started it.

use std::{cell::RefCell, rc::Rc};

use voxear_assets::AssetKind;
use voxear_engine::{Content, LoadStatus, summarise};
use voxear_ui::{ItemFns, NodeId, Ui};

/// What a click in the panel asks the editor to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LevelAction {
    /// Open a level to edit, or while playing, change to it.
    Open(String),
    /// Open a scene that is in no level, on its own.
    OpenScene(String),
    /// Open a level and play it.
    Play(String),
}

#[derive(Clone, Debug)]
enum Row {
    Heading(&'static str),
    Level { path: String, detail: String },
    Scene { path: String },
}

impl Row {
    fn path(&self) -> Option<&str> {
        match self {
            Row::Heading(_) => None,
            Row::Level { path, .. } | Row::Scene { path } => Some(path),
        }
    }

    fn title(&self) -> &str {
        match self {
            Row::Heading(title) => title,
            Row::Level { path, .. } | Row::Scene { path } => {
                let name = path.rsplit('/').next().unwrap_or(path);
                name.rsplit_once('.').map_or(name, |(stem, _)| stem)
            }
        }
    }
}

/// The rows, as shown after the filter, and the current level to mark.
#[derive(Default)]
struct Shown {
    rows: Vec<Row>,
    current: Option<String>,
    /// Each row's play button, and the level it plays, as last bound.
    buttons: Vec<(NodeId, Option<String>)>,
}

pub struct LevelsPanel {
    list: NodeId,
    filter: NodeId,
    progress: NodeId,
    progress_text: NodeId,
    progress_fill: NodeId,
    every: Vec<Row>,
    shown: Rc<RefCell<Shown>>,
    /// The fill's width as last set, so it only restyles when it moves.
    shown_progress: Option<u32>,
}

impl LevelsPanel {
    pub fn new(ui: &mut Ui, document: NodeId, content: &mut Content) -> Self {
        let find = |ui: &Ui, part: &str| ui.find(document, part).unwrap_or_else(|| panic!("Editor.vui has no {part}"));
        let shown: Rc<RefCell<Shown>> = Rc::default();
        let (bound, made) = (shown.clone(), shown.clone());
        let items = ItemFns {
            make: move |ui: &mut Ui, row: NodeId| {
                let icon = ui.icon(row, "level");
                ui.add_class(icon, "row-icon");
                let title = ui.add_label(row, "");
                ui.add_class(title, "row-name");
                let detail = ui.add_label(row, "");
                ui.add_class(detail, "row-count");
                let play = ui.icon_button(row, "play");
                ui.add_class(play, "level-play");
                made.borrow_mut().buttons.push((play, None));
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let mut shown = bound.borrow_mut();
                let Some(item) = shown.rows.get(index).cloned() else { return };
                let parts = ui.children(row);
                let heading = matches!(item, Row::Heading(_));
                ui.set_class(row, "level-heading", heading);
                ui.set_visible(parts[0], !heading);
                ui.set_icon(parts[0], if matches!(item, Row::Scene { .. }) { "scene" } else { "level" });
                ui.set_text(parts[1], item.title());
                let detail = match &item {
                    Row::Level { detail, .. } => detail.as_str(),
                    _ => "",
                };
                ui.set_text(parts[2], detail);
                let current = item.path().is_some() && item.path() == shown.current.as_deref();
                ui.set_class(row, "level-current", current);
                let plays = matches!(item, Row::Level { .. }).then(|| item.path().unwrap_or_default().to_string());
                ui.set_visible(parts[3], plays.is_some());
                if let Some(slot) = shown.buttons.iter_mut().find(|(button, _)| *button == parts[3]) {
                    slot.1 = plays;
                }
            },
        };
        let list = find(ui, "levels-list");
        ui.bind_list(list, 0, items);
        let mut panel = Self {
            list,
            filter: find(ui, "levels-filter"),
            progress: find(ui, "levels-progress"),
            progress_text: find(ui, "levels-progress-text"),
            progress_fill: find(ui, "levels-progress-fill"),
            every: rows(content),
            shown,
            shown_progress: None,
        };
        ui.set_visible(panel.progress, false);
        panel.apply_filter(ui);
        panel
    }

    /// Lists the levels and scenes again, after files came, went or
    /// changed, keeping the filter.
    pub fn refresh(&mut self, ui: &mut Ui, content: &mut Content) {
        self.every = rows(content);
        self.apply_filter(ui);
    }

    /// Follows the filter, the current level and any level change under
    /// way, and returns what a click asked for.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        current: Option<&str>,
        loading: Option<(&str, &LoadStatus)>,
    ) -> Option<LevelAction> {
        if ui.changed(self.filter) {
            self.apply_filter(ui);
        }
        if self.shown.borrow().current.as_deref() != current {
            self.shown.borrow_mut().current = current.map(str::to_string);
            ui.refresh_list(self.list);
        }
        self.show_progress(ui, loading);

        let played =
            self.shown.borrow().buttons.iter().find(|(button, _)| ui.clicked(*button)).and_then(|(_, l)| l.clone());
        if let Some(level) = played {
            return Some(LevelAction::Play(level));
        }
        if !ui.selection_changed(self.list) {
            return None;
        }
        let index = *ui.selection(self.list).first()?;
        ui.set_selection(self.list, &[]);
        match self.shown.borrow().rows.get(index)? {
            Row::Heading(_) => None,
            Row::Level { path, .. } => Some(LevelAction::Open(path.clone())),
            Row::Scene { path } => Some(LevelAction::OpenScene(path.clone())),
        }
    }

    fn apply_filter(&mut self, ui: &mut Ui) {
        let query = ui.text(self.filter).to_lowercase();
        let rows: Vec<Row> = if query.trim().is_empty() {
            self.every.clone()
        } else {
            self.every
                .iter()
                .filter(|row| row.path().is_some_and(|path| path.to_lowercase().contains(query.trim())))
                .cloned()
                .collect()
        };
        let count = rows.len();
        self.shown.borrow_mut().rows = rows;
        ui.set_list_count(self.list, count);
        ui.refresh_list(self.list);
    }

    fn show_progress(&mut self, ui: &mut Ui, loading: Option<(&str, &LoadStatus)>) {
        let percent = loading.map(|(_, status)| (status.progress * 100.0).round() as u32);
        if percent == self.shown_progress {
            return;
        }
        self.shown_progress = percent;
        ui.set_visible(self.progress, loading.is_some());
        let Some((level, status)) = loading else { return };
        let name = level.rsplit('/').next().unwrap_or(level);
        let name = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let waiting = if status.state == voxear_engine::LoadState::Ready { ", waiting to be let in" } else { "" };
        ui.set_text(self.progress_text, &format!("Loading {name}{waiting}"));
        ui.set_inline_style(self.progress_fill, &format!("width: {}%", percent.unwrap_or_default()));
    }
}

/// Every level, then every scene that is in none of them.
fn rows(content: &mut Content) -> Vec<Row> {
    let mut levels: Vec<String> =
        content.db().entries().filter(|e| e.kind == AssetKind::Level).map(|e| e.path.clone()).collect();
    levels.sort();
    let mut scenes: Vec<String> =
        content.db().entries().filter(|e| e.kind == AssetKind::Scene).map(|e| e.path.clone()).collect();
    scenes.sort();
    let mut rows = vec![Row::Heading("Levels")];
    let mut in_a_level = Vec::new();
    for path in levels {
        let detail = match summarise(content, &path) {
            Ok(summary) => {
                in_a_level.extend(summary.scenes.iter().cloned());
                let count = summary.scenes.len();
                let scenes = format!("{count} {}", if count == 1 { "scene" } else { "scenes" });
                match summary.stands_on {
                    Some(base) => format!("{scenes} · on {}", Row::Scene { path: base }.title()),
                    None => scenes,
                }
            }
            Err(error) => format!("unreadable: {error}"),
        };
        rows.push(Row::Level { path, detail });
    }
    let loose: Vec<Row> =
        scenes.into_iter().filter(|s| !in_a_level.contains(s)).map(|path| Row::Scene { path }).collect();
    if !loose.is_empty() {
        rows.push(Row::Heading("Scenes in no level"));
        rows.extend(loose);
    }
    rows
}
