//! What the game adds to the side bar, the strip down the editor's left
//! edge, under Levels: a button for each `[SideBarButton]` method of its
//! scripts, which runs it as a command runs, or for methods sharing a menu
//! one button that opens a menu of them; then each `SideBarItem`, built in
//! the side bar as the game's windows are built in their panels (see
//! `script_windows`). A button's title shows when the pointer rests on it.
//! Both are built again whenever the scripts change.

use voxear_engine::script_types::{ScriptSideBar, SideBarButton};
use voxear_ui::{MenuItem, NodeId, Style};

use super::Editor;

/// What one of the game's buttons on the side bar does: runs its method,
/// or opens a menu of methods.
enum Action {
    Run(SideBarButton),
    Menu(Vec<SideBarButton>),
}

#[derive(Default)]
pub(super) struct SideBar {
    /// What the buttons and items were built from.
    built: ScriptSideBar,
    /// Each button, what its tooltip says, and what it does.
    buttons: Vec<(NodeId, String, Action)>,
    /// The panel each item is built in.
    items: Vec<NodeId>,
    /// The menu open, the button it opened from, and its lines.
    menu: Option<(NodeId, Vec<SideBarButton>)>,
}

impl Editor {
    /// Builds the game's side bar again when the scripts changed, and runs
    /// what was clicked on it.
    pub(super) fn update_side_bar(&mut self) {
        if self.inspector.types().side_bar != self.side_bar.built {
            let wanted = self.inspector.types().side_bar.clone();
            self.build_side_bar(wanted);
        }
        if let Some((owner, lines)) = &self.side_bar.menu
            && let Some(choice) = self.ui.menu_choice(*owner)
        {
            let line = lines.get(choice).cloned();
            self.side_bar.menu = None;
            if let Some(line) = line {
                self.run_script_command(&line.type_name, &line.method);
            }
            return;
        }
        let Some(index) = self.side_bar.buttons.iter().position(|(button, ..)| self.ui.clicked(*button)) else { return };
        let (button, _, action) = &self.side_bar.buttons[index];
        match action {
            Action::Run(line) => {
                let line = line.clone();
                self.run_script_command(&line.type_name, &line.method);
            }
            // Beside the button, as the rail's own open their drawers.
            Action::Menu(lines) => {
                let (button, lines) = (*button, lines.clone());
                let [x, y, width, _] = self.ui.rect(button);
                let items: Vec<MenuItem> = lines.iter().map(|line| MenuItem::new(&line.title)).collect();
                self.ui.open_menu(button, (x + width + 4.0, y), &items);
                self.side_bar.menu = Some((button, lines));
            }
        }
    }

    /// The title of the game's side bar button the pointer rests on, and the
    /// button, for its tooltip.
    pub(super) fn side_bar_tooltip(&self) -> Option<(String, NodeId)> {
        let (button, title, _) = self.side_bar.buttons.iter().find(|(button, ..)| self.ui.is_hovered(*button))?;
        Some((title.clone(), *button))
    }

    /// How many of the game's buttons and items the side bar shows.
    pub fn side_bar_count(&self) -> (usize, usize) {
        (self.side_bar.buttons.len(), self.side_bar.items.len())
    }

    /// Takes away what the side bar had of the game's, and builds what
    /// `wanted` says: a button for each method alone, then one for each
    /// menu, in the order their first method comes, then the items.
    fn build_side_bar(&mut self, wanted: ScriptSideBar) {
        for (button, ..) in std::mem::take(&mut self.side_bar.buttons) {
            self.ui.remove(button);
        }
        self.close_rail_items();
        self.side_bar.items.clear();
        self.side_bar.menu = None;
        let slot = self.nodes.rail_scripts;
        let mut groups: Vec<(Option<String>, Vec<SideBarButton>)> = Vec::new();
        for line in &wanted.buttons {
            match groups.iter_mut().find(|(menu, _)| line.menu.is_some() && *menu == line.menu) {
                Some((_, lines)) => lines.push(line.clone()),
                None => groups.push((line.menu.clone(), vec![line.clone()])),
            }
        }
        for (menu, lines) in groups {
            let button = self.ui.icon_button(slot, &lines[0].icon);
            self.ui.add_class(button, "rail-button");
            let (title, action) = match menu {
                Some(menu) => (menu, Action::Menu(lines)),
                None => (lines[0].title.clone(), Action::Run(lines[0].clone())),
            };
            self.side_bar.buttons.push((button, title, action));
        }
        for type_name in &wanted.items {
            let content = self.ui.panel(slot, Style::column());
            self.ui.add_class(content, "rail-item");
            self.open_rail_item(type_name, content);
            self.side_bar.items.push(content);
        }
        self.side_bar.built = wanted;
    }
}
