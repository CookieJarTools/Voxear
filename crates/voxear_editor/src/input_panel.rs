//! The Input Actions window, in two views. Actions edits the project's
//! actions (see `input_editor`). Layout shows them as key maps, the
//! keyboard and mouse or a gamepad drawn with each bound control lit and
//! labelled with its actions, as a game's key binding screen shows them
//! (see `voxear_engine::keymaps`). A bar over the map picks the device, the
//! map whose actions show, whether keys are coloured by being assigned or
//! by their actions' categories, and whether hidden actions are labelled
//! too; a legend under it says what each colour means. It shows the
//! project's `.inputactions` as the game reads them, and follows changes to
//! the file.

use voxear_engine::keymaps::{self, KeymapColors, KeymapOptions};
use voxear_input::{Input, Scheme};
use voxear_ui::{KeyState, NodeId, Style, Ui};

use crate::input_editor::ActionsEditor;

pub struct InputPanel {
    view: NodeId,
    actions_view: NodeId,
    layout_view: NodeId,
    editor: ActionsEditor,
    scheme: NodeId,
    map: NodeId,
    colors: NodeId,
    hidden: NodeId,
    keymap: NodeId,
    legend: NodeId,
    /// The maps the map menu lists, after All Maps.
    maps: Vec<String>,
    /// What the legend shows, to make it again when that changes.
    legend_for: Option<(KeymapColors, Vec<String>)>,
}

impl InputPanel {
    pub fn new(ui: &mut Ui, panel: NodeId) -> Self {
        let tabs = ui.panel(panel, Style::row());
        ui.add_class(tabs, "input-bar");
        let view = ui.segmented(tabs);
        ui.add_segment(view, "Actions", None);
        ui.add_segment(view, "Layout", None);
        ui.set_name(view, "input-view");
        let actions_view = ui.panel(panel, Style::default());
        ui.add_class(actions_view, "fill");
        ui.add_class(actions_view, "input-view");
        let editor = ActionsEditor::new(ui, actions_view);
        let layout_view = ui.panel(panel, Style::default());
        ui.add_class(layout_view, "fill");
        ui.add_class(layout_view, "input-view");
        ui.set_visible(layout_view, false);
        let panel = layout_view;
        let bar = ui.panel(panel, Style::row());
        ui.add_class(bar, "input-bar");
        let scheme = ui.segmented(bar);
        ui.add_segment(scheme, "Keyboard", Some("select"));
        ui.add_segment(scheme, "Gamepad", Some("gamepad"));
        ui.set_name(scheme, "input-scheme");
        let map = ui.dropdown(bar, &["All Maps"], 0);
        ui.set_name(map, "input-map");
        let spacer = ui.panel(bar, Style::default());
        ui.add_class(spacer, "spacer");
        let colors = ui.segmented(bar);
        ui.add_segment(colors, "Assigned", None);
        ui.add_segment(colors, "Categories", None);
        ui.set_name(colors, "input-colors");
        let hidden = ui.toggle(bar, "Hidden", false);
        ui.add_class(hidden, "checkbox");
        ui.set_name(hidden, "input-hidden");
        let keymap = ui.keymap(panel);
        ui.set_name(keymap, "input-keymap");
        let legend = ui.panel(panel, Style::row());
        ui.add_class(legend, "input-legend");
        Self {
            view,
            actions_view,
            layout_view,
            editor,
            scheme,
            map,
            colors,
            hidden,
            keymap,
            legend,
            maps: Vec::new(),
            legend_for: None,
        }
    }

    /// Whether the Actions view waits for a key or button for a binding.
    pub fn is_listening(&self) -> bool {
        self.editor.is_listening()
    }

    /// Shows the view picked, once a frame while the window shows. Gives
    /// the actions file's new text after the Actions view changed it.
    pub fn update(&mut self, ui: &mut Ui, input: &mut Input) -> Option<String> {
        let actions = ui.segment(self.view) == 0;
        ui.set_visible(self.actions_view, actions);
        ui.set_visible(self.layout_view, !actions);
        if actions {
            return self.editor.update(ui, input);
        }
        self.show_layout(ui, input);
        None
    }

    /// Shows the game's input as the bar picks it. The key map makes its
    /// keys again only when they change.
    fn show_layout(&mut self, ui: &mut Ui, input: &Input) {
        let maps = input.maps();
        if maps != self.maps {
            let picked = self.picked_map(ui);
            let mut options = vec!["All Maps".to_string()];
            options.extend(maps.iter().cloned());
            let labels: Vec<&str> = options.iter().map(String::as_str).collect();
            ui.set_options(self.map, &labels);
            let again = picked.and_then(|map| maps.iter().position(|m| *m == map)).map_or(0, |at| at + 1);
            ui.set_selected_option(self.map, again);
            self.maps = maps;
        }
        let scheme = if ui.segment(self.scheme) == 1 { Scheme::Gamepad } else { Scheme::KeyboardMouse };
        let colors = if ui.segment(self.colors) == 1 { KeymapColors::Categories } else { KeymapColors::Assigned };
        let options = KeymapOptions { colors, show_hidden: ui.is_checked(self.hidden), map: self.picked_map(ui) };
        ui.set_keymap(self.keymap, keymaps::keymap_data(input, scheme, &options));
        let categories: Vec<String> = input.categories().iter().map(|c| format!("{}{}", c.name, c.color)).collect();
        if self.legend_for.as_ref() != Some(&(colors, categories.clone())) {
            self.show_legend(ui, input, colors);
            self.legend_for = Some((colors, categories));
        }
    }

    fn picked_map(&self, ui: &Ui) -> Option<String> {
        ui.selected_option(self.map).checked_sub(1).and_then(|at| self.maps.get(at).cloned())
    }

    fn show_legend(&mut self, ui: &mut Ui, input: &Input, colors: KeymapColors) {
        for child in ui.children(self.legend) {
            ui.remove(child);
        }
        for (name, color, state) in keymaps::legend(input, colors) {
            let entry = ui.panel(self.legend, Style::row());
            ui.add_class(entry, "input-legend-entry");
            let swatch = ui.panel(entry, Style::default());
            ui.add_class(swatch, "keymap-key");
            ui.add_class(swatch, "input-legend-swatch");
            ui.add_class(
                swatch,
                match state {
                    KeyState::Assigned => "assigned",
                    KeyState::Hidden => "hidden",
                    KeyState::Unused => "unused",
                },
            );
            if let Some(color) = color {
                ui.set_inline_style(swatch, &format!("background-color: {}", color.css()));
            }
            let label = ui.add_label(entry, &name);
            ui.add_class(label, "input-legend-label");
        }
    }
}
