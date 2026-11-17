//! The Project Settings and Preferences windows, built alike from their
//! settings (see `setting_info`): a search across the top, the categories
//! down the left, and on the right the chosen category's sections, each a
//! titled list of rows, a label beside its control, with the section's
//! rarely wanted settings folded under Advanced. Each label's tooltip says
//! what the setting does and its default; a value changed from its default
//! is marked beside its label, and a row's menu sets it back.
//!
//! Searching shows every setting whose label, section, category or tooltip
//! has the words typed, from every category, under its category and
//! section, and the list keeps only the categories with any.
//!
//! The window holds no values of its own: the editor shows them in it each
//! frame it is on screen ([`SettingsWindow::show`]) and does what it asks
//! ([`Action`]). Parts of a window of their own, as the collision matrix,
//! are given a place for the editor to fill ([`SettingsWindow::part`]).

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use serde_json::{Value as Json, json};
use voxear_assets::AssetKind;
use voxear_ui::{Color, ItemFns, MenuItem, NodeId, Style, Ui};

use crate::setting_info::{self, Category, Kind, SectionHelp, Setting, number_text};

/// How long a setting shown by the palette or a call stays marked.
const MARKED: Duration = Duration::from_secs(2);
/// How tall a category's row is in the list.
const CATEGORY_ROW: f32 = 26.0;
/// What a vector's numbers are called, in order.
const AXES: [&str; 4] = ["X", "Y", "Z", "W"];

/// What the window asks of the editor this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A setting changed in place, to a value as `setting_info` keeps it.
    Set(&'static str, Json),
    /// A setting back to its default.
    Reset(&'static str),
    /// An asset to pick for a setting, from a picker under `anchor`.
    PickAsset { key: &'static str, kind: AssetKind, anchor: NodeId },
    /// A colour to pick for a setting, from its swatch.
    PickColor { key: &'static str, swatch: NodeId },
    /// A folder to pick for a setting, from the desktop's chooser.
    PickFolder(&'static str),
    /// A button pressed.
    Press(&'static str),
    /// Every setting of a category back to its default.
    ResetCategory(&'static str),
    /// A page of the manual to open.
    OpenManual(&'static str),
}

/// A section of a category: its title, and where its rows and its advanced
/// rows go.
struct Block {
    category: &'static str,
    section: &'static str,
    root: NodeId,
    heading: NodeId,
    rows: NodeId,
    /// Folded rarely wanted settings, hidden when the section has none.
    advanced: NodeId,
}

struct Row {
    setting: &'static Setting,
    row: NodeId,
    /// The label, or for a part of its own, its section's title, which
    /// carries the tooltip.
    label: NodeId,
    control: Control,
    block: usize,
    tipped: bool,
    shown: Json,
    default: Json,
}

/// A row's control, by what its setting holds.
enum Control {
    Toggle(NodeId),
    Choice(NodeId),
    Number {
        field: NodeId,
        slider: Option<NodeId>,
    },
    Vector(Vec<NodeId>),
    Color(NodeId),
    /// A button naming the folder or asset, which picks another, and one
    /// that clears it.
    Picked {
        pick: NodeId,
        clear: NodeId,
    },
    Text(NodeId),
    /// A row for each item, a field or a dropdown with a button to take it
    /// out, and one to add another.
    List {
        holder: NodeId,
        items: Vec<(NodeId, NodeId)>,
        add: NodeId,
    },
    Info(NodeId),
    Part(NodeId),
    Button(NodeId),
}

pub struct SettingsWindow {
    settings: &'static [Setting],
    categories: &'static [Category],
    search: NodeId,
    list: NodeId,
    /// The categories the list shows now: all of them, or while searching,
    /// those with a setting found.
    listed: Rc<RefCell<Vec<&'static str>>>,
    title: NodeId,
    more: NodeId,
    scroll: NodeId,
    nothing: NodeId,
    blocks: Vec<Block>,
    rows: Vec<Row>,
    category: &'static str,
    query: String,
    marked: Option<(NodeId, Instant)>,
    /// A setting to scroll to once the window is on screen, as it is not
    /// laid out until its panel is in front.
    revealing: Option<&'static str>,
    /// The list's selection as the window last set it, so a selection the
    /// window made is not taken for a click.
    selected: Option<usize>,
    /// Advanced foldouts a search opened, to close again after it.
    opened: Vec<NodeId>,
}

impl SettingsWindow {
    /// Builds a window of `settings` into `root`, named from `name` for
    /// tests and tools to find (`<name>-search`), with the first of
    /// `categories` showing.
    pub fn new(
        ui: &mut Ui,
        root: NodeId,
        name: &str,
        (settings, categories, help): (&'static [Setting], &'static [Category], &'static [SectionHelp]),
    ) -> Self {
        ui.add_class(root, "settings-window");
        let bar = ui.panel(root, Style::row());
        ui.add_class(bar, "settings-window-bar");
        let field = ui.panel(bar, Style::row());
        ui.add_class(field, "settings-search");
        let glass = ui.icon(field, "search");
        ui.add_class(glass, "settings-search-icon");
        let search = ui.text_field(field, "");
        ui.add_class(search, "search-field");
        ui.set_placeholder(search, "Search settings");
        ui.set_name(search, &format!("{name}-search"));

        let main = ui.panel(root, Style::row());
        ui.add_class(main, "settings-window-main");
        let side = ui.panel(main, Style::column());
        ui.add_class(side, "settings-categories");
        let listed = Rc::new(RefCell::new(categories.iter().map(|category| category.name).collect::<Vec<_>>()));
        let (shown, abouts) = (listed.clone(), categories);
        let items = ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                let label = ui.add_label(row, "");
                ui.add_class(label, "settings-category");
            },
            bind: move |ui: &mut Ui, row: NodeId, index: usize| {
                let name = shown.borrow().get(index).copied().unwrap_or_default();
                let about = abouts.iter().find(|category| category.name == name).map_or("", |category| category.about);
                if let Some(label) =
                    ui.children(row).into_iter().find(|&child| ui.has_class(child, "settings-category"))
                {
                    ui.set_text(label, name);
                    ui.set_tooltip(label, about, None);
                }
            },
        };
        let list = ui.list_view(side, CATEGORY_ROW, categories.len(), items);
        ui.add_class(list, "settings-category-list");
        ui.set_name(list, &format!("{name}-categories"));
        ui.set_selection(list, &[0]);

        let detail = ui.panel(main, Style::column());
        ui.add_class(detail, "settings-detail");
        let head = ui.panel(detail, Style::row());
        ui.add_class(head, "settings-head");
        let title = ui.add_label(head, categories[0].name);
        ui.add_class(title, "settings-title");
        ui.set_name(title, &format!("{name}-title"));
        let more = ui.icon_button(head, "more");
        ui.add_class(more, "icon-button");
        ui.set_tooltip(more, "Reset this category, or read about it", None);
        let scroll = ui.panel(detail, Style::column().scroll());
        ui.add_class(scroll, "settings-scroll");
        let content = ui.panel(scroll, Style::column());
        ui.add_class(content, "settings-content");
        let nothing = ui.add_label(content, "No settings match");
        ui.add_class(nothing, "settings-nothing");
        ui.set_visible(nothing, false);

        let mut blocks: Vec<Block> = Vec::new();
        let mut rows = Vec::new();
        for setting in settings {
            let block = match blocks.iter().position(|b| b.category == setting.category && b.section == setting.section)
            {
                Some(at) => at,
                None => {
                    let help = help.iter().find(|h| h.category == setting.category && h.section == setting.section);
                    blocks.push(block(ui, content, setting, help.map(|help| help.help)));
                    blocks.len() - 1
                }
            };
            let parent = if setting.advanced {
                ui.set_visible(blocks[block].advanced, true);
                ui.foldout_content(blocks[block].advanced).unwrap_or(blocks[block].rows)
            } else {
                blocks[block].rows
            };
            let (row, label, control) = build_row(ui, parent, setting, blocks[block].heading);
            ui.set_name(row, &format!("setting-{}", setting.key));
            rows.push(Row {
                setting,
                row,
                label,
                control,
                block,
                tipped: false,
                shown: Json::Null,
                default: Json::Null,
            });
        }
        let category = categories[0].name;
        let mut window = Self {
            settings,
            categories,
            search,
            list,
            listed,
            title,
            more,
            scroll,
            nothing,
            blocks,
            rows,
            category,
            query: String::new(),
            marked: None,
            revealing: None,
            selected: None,
            opened: Vec::new(),
        };
        window.apply_filter(ui);
        window
    }

    /// The category showing.
    pub fn category(&self) -> &'static str {
        self.category
    }

    /// Where a part of its own goes (see [`Kind::Part`]), for the editor to
    /// fill.
    pub fn part(&self, key: &str) -> Option<NodeId> {
        self.rows.iter().find(|row| row.setting.key == key).and_then(|row| match row.control {
            Control::Part(holder) => Some(holder),
            _ => None,
        })
    }

    /// The row of the setting whose key this is, for tests and tools.
    pub fn row(&self, key: &str) -> Option<NodeId> {
        self.rows.iter().find(|row| row.setting.key == key).map(|row| row.row)
    }

    /// Shows a category, its search cleared.
    pub fn select(&mut self, ui: &mut Ui, category: &str) {
        let Some(found) = self.categories.iter().find(|c| c.name.eq_ignore_ascii_case(category)) else { return };
        self.category = found.name;
        ui.set_text(self.search, "");
        self.apply_filter(ui);
    }

    /// Shows a setting once the window is on screen: its category, scrolled
    /// to it and marked for a moment, a number's field focused to type in.
    pub fn reveal(&mut self, ui: &mut Ui, key: &'static str) {
        self.show_category_of(ui, key);
        self.revealing = Some(key);
    }

    /// Takes what was done in the window this frame.
    pub fn update(&mut self, ui: &mut Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        if ui.changed(self.search) {
            self.apply_filter(ui);
        }
        // A category clicked, or clicked again while searching, shows it with the search cleared.
        let at = ui.selection(self.list).first().copied();
        let clicked = ui.selection_changed(self.list) && at != self.selected;
        if (clicked || (ui.clicked(self.list) && !self.query.is_empty()))
            && let Some(category) = at.and_then(|at| self.listed.borrow().get(at).copied())
        {
            self.category = category;
            ui.set_text(self.search, "");
            self.apply_filter(ui);
        }
        if ui.clicked(self.more) {
            let [x, y, _, h] = ui.rect(self.more);
            let items = [MenuItem::new("Reset Category"), MenuItem::new("Open Manual Page")];
            ui.open_menu(self.more, (x, y + h), &items);
        }
        match ui.menu_choice(self.more) {
            Some(0) => actions.push(Action::ResetCategory(self.category)),
            Some(_) => {
                let manual = self.categories.iter().find(|c| c.name == self.category).map(|c| c.manual);
                actions.extend(manual.map(Action::OpenManual));
            }
            None => {}
        }
        if let Some(key) = self.revealing.filter(|_| ui.rect(self.scroll)[3] > 0.0) {
            self.revealing = None;
            self.show_setting(ui, key);
        }
        if let Some((row, at)) = self.marked
            && at.elapsed() > MARKED
        {
            ui.set_class(row, "found", false);
            self.marked = None;
        }
        for row in &self.rows {
            if !ui.is_visible(row.row) {
                continue;
            }
            if row.setting.settable() && ui.context_clicked(row.row) {
                let at = ui.pointer_position().unwrap_or_default();
                let changed = !setting_info::same(&row.shown, &row.default);
                ui.open_menu(row.row, at, &[MenuItem::new("Reset to Default").enabled(changed)]);
            }
            if ui.menu_choice(row.row) == Some(0) {
                actions.push(Action::Reset(row.setting.key));
            }
            actions.extend(read(ui, row));
        }
        actions
    }

    /// Shows each setting's value and default, as `values` gives them by
    /// key, but in a field being typed in: a changed value marked, and each
    /// label's tooltip with its default.
    pub fn show(&mut self, ui: &mut Ui, values: &dyn Fn(&str) -> Option<(Json, Json)>) {
        for row in &mut self.rows {
            let Some((value, default)) = values(row.setting.key) else {
                if !row.tipped {
                    ui.set_tooltip(row.label, &setting_info::tooltip(row.setting, None), None);
                    row.tipped = true;
                }
                continue;
            };
            if !row.tipped || !setting_info::same(&row.default, &default) {
                ui.set_tooltip(row.label, &setting_info::tooltip(row.setting, Some(&default)), None);
                row.tipped = true;
            }
            let changed = row.setting.settable() && !setting_info::same(&value, &default);
            if ui.has_class(row.row, "changed") != changed {
                ui.set_class(row.row, "changed", changed);
            }
            row.default = default;
            show_value(ui, row.setting, &mut row.control, &value);
            row.shown = value;
        }
    }

    /// Shows the category a setting is in, its Advanced open where it is
    /// an advanced one, to be laid out by the next frame.
    fn show_category_of(&mut self, ui: &mut Ui, key: &str) {
        let Some(row) = self.rows.iter().find(|row| row.setting.key == key) else { return };
        let (category, advanced) =
            (row.setting.category, row.setting.advanced.then_some(self.blocks[row.block].advanced));
        self.select(ui, category);
        if let Some(foldout) = advanced.filter(|&foldout| !ui.is_open(foldout)) {
            ui.set_open(foldout, true);
        }
    }

    /// Scrolls to a setting shown, marks it for a moment and focuses a
    /// number's field.
    fn show_setting(&mut self, ui: &mut Ui, key: &str) {
        let Some(at) = self.rows.iter().position(|row| row.setting.key == key) else { return };
        let row = &self.rows[at];
        if let Some((old, _)) = self.marked.take() {
            ui.set_class(old, "found", false);
        }
        ui.set_class(row.row, "found", true);
        self.marked = Some((row.row, Instant::now()));
        let [_, top, ..] = ui.rect(self.scroll);
        let [_, y, _, height] = ui.rect(row.row);
        let [x, scrolled] = ui.scroll_offset(self.scroll);
        let [_, _, _, shown] = ui.viewport(self.scroll);
        let place = y - top + scrolled;
        if place < scrolled || place + height > scrolled + shown {
            ui.set_scroll(self.scroll, [x, (place - shown / 3.0).max(0.0)]);
        }
        if let Control::Number { field, .. } = row.control {
            ui.focus(field);
        }
    }

    /// Shows the chosen category's sections, or while searching, every
    /// setting found under its category's and section's names, and lists
    /// the categories there are any in.
    fn apply_filter(&mut self, ui: &mut Ui) {
        self.query = ui.text(self.search).trim().to_string();
        let searching = !self.query.is_empty();
        for foldout in self.opened.drain(..) {
            ui.set_open(foldout, false);
        }
        let mut shown_blocks = vec![false; self.blocks.len()];
        let mut found_in: Vec<&'static str> = Vec::new();
        for row in &self.rows {
            let shown = if searching { row.setting.found(&self.query) } else { row.setting.category == self.category };
            ui.set_visible(row.row, shown);
            shown_blocks[row.block] |= shown;
            if shown && searching {
                if !found_in.contains(&row.setting.category) {
                    found_in.push(row.setting.category);
                }
                let advanced = self.blocks[row.block].advanced;
                if row.setting.advanced && !ui.is_open(advanced) {
                    ui.set_open(advanced, true);
                    self.opened.push(advanced);
                }
            }
        }
        for (block, shown) in self.blocks.iter().zip(&shown_blocks) {
            ui.set_visible(block.root, *shown);
            let heading =
                if searching { format!("{} ▸ {}", block.category, block.section) } else { block.section.to_string() };
            ui.set_text(block.heading, &heading);
        }
        ui.set_visible(self.nothing, searching && found_in.is_empty());
        ui.set_text(self.title, if searching { "Search" } else { self.category });
        ui.set_visible(self.more, !searching);
        let listed: Vec<&'static str> = match searching {
            true => self.categories.iter().map(|c| c.name).filter(|name| found_in.contains(name)).collect(),
            false => self.categories.iter().map(|c| c.name).collect(),
        };
        self.selected = listed.iter().position(|&name| name == self.category);
        *self.listed.borrow_mut() = listed;
        ui.set_list_count(self.list, self.listed.borrow().len());
        ui.refresh_list(self.list);
        ui.set_selection(self.list, &self.selected.into_iter().collect::<Vec<_>>());
        if !searching {
            ui.set_scroll(self.scroll, [0.0, 0.0]);
        }
    }

    /// The settings of a category, as the window lists them.
    pub fn settings_of<'a>(&self, category: &'a str) -> impl Iterator<Item = &'static Setting> + 'a {
        self.settings.iter().filter(move |setting| setting.category == category)
    }
}

/// A section's block: its title, a line of help where it has one, its rows
/// and its Advanced foldout.
fn block(ui: &mut Ui, content: NodeId, setting: &Setting, help: Option<&str>) -> Block {
    let root = ui.panel(content, Style::column());
    ui.add_class(root, "settings-section");
    let heading = ui.add_label(root, setting.section);
    ui.add_class(heading, "settings-group");
    if let Some(help) = help {
        let line = ui.add_label(root, help);
        ui.add_class(line, "settings-help");
    }
    let rows = ui.panel(root, Style::column());
    ui.add_class(rows, "settings-rows");
    let advanced = ui.foldout(root, "Advanced", false);
    ui.add_class(advanced, "settings-advanced");
    ui.set_visible(advanced, false);
    Block { category: setting.category, section: setting.section, root, heading, rows, advanced }
}

/// A setting's row: its label, and the control for what it holds.
fn build_row(ui: &mut Ui, parent: NodeId, setting: &Setting, heading: NodeId) -> (NodeId, NodeId, Control) {
    if setting.kind == Kind::Part {
        let row = ui.panel(parent, Style::column());
        ui.add_class(row, "setting-part");
        return (row, heading, Control::Part(row));
    }
    let row = ui.panel(parent, Style::default());
    ui.add_class(row, "prop");
    ui.add_class(row, "setting");
    let label = ui.add_label(row, setting.label);
    ui.add_class(label, "prop-label");
    let value_holder = |ui: &mut Ui| {
        let holder = ui.panel(row, Style::row());
        ui.add_class(holder, "prop-value");
        holder
    };
    let unit_label = |ui: &mut Ui, holder: NodeId, unit: &str| {
        if !unit.is_empty() {
            let unit = ui.add_label(holder, unit);
            ui.add_class(unit, "setting-unit");
        }
    };
    let control = match setting.kind {
        Kind::Toggle => {
            let toggle = ui.toggle(row, "", false);
            ui.add_class(toggle, "checkbox");
            Control::Toggle(toggle)
        }
        Kind::Choice(options) => {
            let labels: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
            let dropdown = ui.dropdown(row, &labels, 0);
            ui.add_class(dropdown, "value-choice");
            Control::Choice(dropdown)
        }
        Kind::Number { min, max, unit, whole, slider } => {
            ui.add_class(label, "scrub");
            let holder = value_holder(ui);
            let slider = slider.then(|| {
                let slider = ui.slider(holder, min as f32, max as f32, min as f32);
                ui.add_class(slider, "setting-slider");
                // The field beside it says the value, and takes one typed.
                let parts = ui.children(slider);
                if let Some(&value) = parts.iter().find(|&&part| ui.has_class(part, "slider-value")) {
                    ui.set_visible(value, false);
                }
                if whole {
                    ui.set_slider_step(slider, 1.0);
                }
                slider
            });
            let field = ui.text_field(holder, "");
            ui.add_class(field, "value-input");
            ui.add_class(field, "number-input");
            if slider.is_some() {
                ui.add_class(field, "setting-slider-field");
            }
            unit_label(ui, holder, unit);
            Control::Number { field, slider }
        }
        Kind::Vector { count, unit } => {
            let holder = value_holder(ui);
            ui.add_class(holder, "setting-vector");
            let fields = (0..count)
                .map(|at| {
                    let axis = ui.add_label(holder, AXES[at.min(3)]);
                    ui.add_class(axis, "setting-axis");
                    let field = ui.text_field(holder, "");
                    ui.add_class(field, "value-input");
                    ui.add_class(field, "number-input");
                    field
                })
                .collect();
            unit_label(ui, holder, unit);
            Control::Vector(fields)
        }
        Kind::Color => {
            let swatch = ui.panel(row, Style::default());
            ui.add_class(swatch, "swatch");
            Control::Color(swatch)
        }
        Kind::Folder | Kind::Asset(_) => {
            let holder = value_holder(ui);
            let pick = ui.button(holder, "None");
            ui.add_class(pick, "setting-folder");
            let clear = ui.icon_button(holder, "close");
            ui.add_class(clear, "setting-clear");
            ui.set_tooltip(clear, "Set to none", None);
            Control::Picked { pick, clear }
        }
        Kind::Text => {
            let field = ui.text_field(row, "");
            ui.add_class(field, "value-input");
            Control::Text(field)
        }
        Kind::Words | Kind::Picks(_) => {
            ui.add_class(row, "setting-list-row");
            let holder = ui.panel(row, Style::column());
            ui.add_class(holder, "setting-list");
            let add = ui.button(holder, "Add");
            ui.add_class(add, "settings-button");
            Control::List { holder, items: Vec::new(), add }
        }
        Kind::Info => {
            let text = ui.add_label(row, "");
            ui.add_class(text, "setting-info");
            Control::Info(text)
        }
        Kind::Button(words) => {
            let button = ui.button(row, words);
            ui.add_class(button, "settings-button");
            Control::Button(button)
        }
        Kind::Part => unreachable!("a part has no row"),
    };
    (row, label, control)
}

/// A number typed, with the thousands it may be written in.
fn typed_number(ui: &Ui, field: NodeId) -> Option<f64> {
    ui.text(field).trim().replace([',', ' ', '_'], "").parse::<f64>().ok().filter(|n| n.is_finite())
}

/// What was changed in a row's control this frame, kept within its range.
fn read(ui: &Ui, row: &Row) -> Option<Action> {
    let key = row.setting.key;
    let set = |value: Json| Some(Action::Set(key, value));
    match (&row.control, row.setting.kind) {
        (Control::Toggle(toggle), _) if ui.changed(*toggle) => set(json!(ui.is_checked(*toggle))),
        (Control::Choice(dropdown), Kind::Choice(options)) if ui.changed(*dropdown) => {
            set(json!(options.get(ui.selected_option(*dropdown)).map_or("", |(label, _)| *label)))
        }
        (Control::Number { field, slider }, Kind::Number { min, max, whole, .. }) => {
            let typed = if ui.changed(*field) {
                typed_number(ui, *field)
            } else {
                slider.filter(|slider| ui.changed(*slider)).map(|slider| f64::from(ui.slider_value(slider)))
            }?;
            let number = typed.clamp(min, max);
            set(if whole { json!(number.round() as i64) } else { json!(number) })
        }
        (Control::Vector(fields), _) if fields.iter().any(|&field| ui.changed(field)) => {
            let numbers: Option<Vec<f64>> = fields.iter().map(|&field| typed_number(ui, field)).collect();
            set(json!(numbers?))
        }
        (Control::Color(swatch), _) if ui.clicked(*swatch) => Some(Action::PickColor { key, swatch: *swatch }),
        (Control::Picked { pick, .. }, Kind::Folder) if ui.clicked(*pick) => Some(Action::PickFolder(key)),
        (Control::Picked { pick, .. }, Kind::Asset(kind)) if ui.clicked(*pick) => {
            Some(Action::PickAsset { key, kind, anchor: *pick })
        }
        (Control::Picked { clear, .. }, _) if ui.clicked(*clear) => set(Json::Null),
        (Control::Text(field), _) if ui.changed(*field) => set(json!(ui.text(*field).trim())),
        (Control::List { items, add, .. }, kind) => {
            let mut list: Vec<Json> = row.shown.as_array().cloned().unwrap_or_default();
            if ui.clicked(*add) {
                let next = match kind {
                    Kind::Picks(options) => {
                        options.iter().map(|(label, _)| *label).find(|label| !list.contains(&json!(label)))
                    }
                    _ => Some(""),
                };
                list.push(json!(next?));
                return set(Json::Array(list));
            }
            for (at, &(item, remove)) in items.iter().enumerate() {
                if ui.clicked(remove) && at < list.len() {
                    list.remove(at);
                    return set(Json::Array(list));
                }
                if ui.changed(item) && at < list.len() {
                    list[at] = match kind {
                        Kind::Picks(options) => {
                            json!(options.get(ui.selected_option(item)).map_or("", |(label, _)| *label))
                        }
                        _ => json!(ui.text(item).trim()),
                    };
                    return set(Json::Array(list));
                }
            }
            None
        }
        (Control::Button(button), _) if ui.clicked(*button) => Some(Action::Press(key)),
        _ => None,
    }
}

/// Shows a value in a row's control, but in a field being typed in.
fn show_value(ui: &mut Ui, setting: &Setting, control: &mut Control, value: &Json) {
    let typing = |ui: &Ui, field: NodeId| ui.focused() == Some(field);
    let put_text = |ui: &mut Ui, field: NodeId, text: &str| {
        if !typing(ui, field) && ui.text(field) != text {
            ui.set_text(field, text);
        }
    };
    match (control, setting.kind) {
        (Control::Toggle(toggle), _) => {
            let on = value.as_bool().unwrap_or_default();
            if ui.is_checked(*toggle) != on {
                ui.set_checked(*toggle, on);
            }
        }
        (Control::Choice(dropdown), Kind::Choice(options)) => {
            let word = value.as_str().unwrap_or_default();
            let at = options.iter().position(|(label, _)| *label == word).unwrap_or(0);
            if ui.selected_option(*dropdown) != at {
                ui.set_selected_option(*dropdown, at);
            }
        }
        (Control::Number { field, slider }, _) => {
            let number = value.as_f64().unwrap_or_default();
            put_text(ui, *field, &number_text(number));
            if let Some(slider) = slider
                && ui.slider_value(*slider) != number as f32
            {
                ui.set_slider_value(*slider, number as f32);
            }
        }
        (Control::Vector(fields), _) => {
            let numbers = value.as_array().cloned().unwrap_or_default();
            for (field, number) in fields.iter().zip(numbers) {
                put_text(ui, *field, &number_text(number.as_f64().unwrap_or_default()));
            }
        }
        (Control::Color(swatch), _) => {
            let hex = value
                .as_str()
                .and_then(|text| text.strip_prefix('#'))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok());
            let color = Color::hex(hex.unwrap_or_default());
            if ui.style(*swatch).and_then(|style| style.background) != Some(color) {
                ui.set_inline_style(*swatch, &format!("background-color: {}", color.css()));
            }
        }
        (Control::Picked { pick, clear }, _) => {
            let path = value.as_str();
            let name = path.map(|path| path.rsplit(['/', '\\']).next().unwrap_or(path));
            let text = name.unwrap_or("None");
            if ui.text(*pick) != text {
                ui.set_text(*pick, text);
                ui.set_tooltip(*pick, path.unwrap_or("Pick one"), None);
            }
            if ui.is_visible(*clear) != path.is_some() {
                ui.set_visible(*clear, path.is_some());
            }
        }
        (Control::Text(field), _) => put_text(ui, *field, value.as_str().unwrap_or_default()),
        (Control::List { holder, items, add }, kind) => {
            let list = value.as_array().cloned().unwrap_or_default();
            if items.len() != list.len() {
                for (item, _) in items.drain(..) {
                    if let Some(line) = ui.parent(item) {
                        ui.remove(line);
                    }
                }
                for _ in 0..list.len() {
                    let line = ui.panel(*holder, Style::row());
                    ui.add_class(line, "setting-list-item");
                    let item = match kind {
                        Kind::Picks(options) => {
                            let labels: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
                            let dropdown = ui.dropdown(line, &labels, 0);
                            ui.add_class(dropdown, "value-choice");
                            dropdown
                        }
                        _ => {
                            let field = ui.text_field(line, "");
                            ui.add_class(field, "value-input");
                            field
                        }
                    };
                    let remove = ui.icon_button(line, "close");
                    ui.add_class(remove, "setting-clear");
                    ui.set_tooltip(remove, "Take this out", None);
                    items.push((item, remove));
                }
                // The Add button stays last.
                ui.remove(*add);
                *add = ui.button(*holder, "Add");
                ui.add_class(*add, "settings-button");
            }
            for ((item, _), value) in items.iter().zip(&list) {
                let word = value.as_str().unwrap_or_default();
                match kind {
                    Kind::Picks(options) => {
                        let at = options.iter().position(|(label, _)| *label == word).unwrap_or(0);
                        if ui.selected_option(*item) != at {
                            ui.set_selected_option(*item, at);
                        }
                    }
                    _ => put_text(ui, *item, word),
                }
            }
        }
        (Control::Info(text), _) => {
            let words = setting_info::describe(setting, value);
            if ui.text(*text) != words {
                ui.set_text(*text, &words);
            }
        }
        _ => {}
    }
}
