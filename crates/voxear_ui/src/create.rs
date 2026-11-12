//! Elements made from code one at a time, as scripts make them (`new
//! Button("Pause")` in Voxear.Core), each as its markup tag makes it with
//! no attributes.

use crate::{
    Style,
    tree::{Element, Kind, NodeId, Text, Ui},
};

/// What an element made from code is, as its markup tag names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    Panel,
    ScrollView,
    Label,
    Button,
    Toggle,
    Slider,
    TextField,
    Foldout,
    Dropdown,
    Image,
    Icon,
    /// A list whose rows its owner makes and fills, as the game's scripts do.
    ListView,
    TreeView,
    /// A device drawn as its keys, with their actions; see [`crate::keymap`].
    Keymap,
    /// The controls of an action, its text, as glyphs; see [`crate::prompt`].
    Prompt,
    /// A map of the world small on the HUD, and one opened large; see
    /// [`crate::map_view`].
    Minimap,
    Map,
}

impl ElementKind {
    /// Every kind, in the order scripts number them.
    pub const ALL: [ElementKind; 17] = [
        Self::Panel,
        Self::ScrollView,
        Self::Label,
        Self::Button,
        Self::Toggle,
        Self::Slider,
        Self::TextField,
        Self::Foldout,
        Self::Dropdown,
        Self::Image,
        Self::Icon,
        Self::ListView,
        Self::TreeView,
        Self::Keymap,
        Self::Prompt,
        Self::Minimap,
        Self::Map,
    ];

    /// The kind scripts know by this number, or none.
    pub fn from_number(number: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(number).ok()?).copied()
    }
}

impl Ui {
    /// Makes an element of `kind` at the end of `parent`, or of its content
    /// when it is a foldout. `text` is a label's or button's text, a
    /// toggle's label, a field's value, a foldout's title or an icon's name;
    /// for a slider, its least and greatest values and its step, as "0 1 0";
    /// and for a dropdown, its options, one a line.
    pub fn create(&mut self, parent: NodeId, kind: ElementKind, text: &str) -> NodeId {
        let parent = self.foldout_content(parent).unwrap_or(parent);
        match kind {
            ElementKind::Panel => self.add(parent, Element::new(Kind::Panel, Style::column())),
            ElementKind::ScrollView => self.add(parent, Element::new(Kind::ScrollView, Style::column().scroll())),
            ElementKind::Label | ElementKind::Button => {
                let kind = if kind == ElementKind::Label { Kind::Label } else { Kind::Button };
                self.add(parent, Element { text: Some(Text::new(text)), ..Element::new(kind, Style::default()) })
            }
            ElementKind::Toggle => self.toggle(parent, text, false),
            ElementKind::Slider => {
                let numbers: Vec<f32> = text.split_whitespace().filter_map(|n| n.parse().ok()).collect();
                let (min, max, step) = match numbers[..] {
                    [min, max, step, ..] => (min, max, step),
                    [min, max] => (min, max, 0.0),
                    _ => (0.0, 1.0, 0.0),
                };
                let node = self.slider(parent, min, max, min);
                self.set_slider_step(node, step);
                node
            }
            ElementKind::TextField => self.text_field(parent, text),
            ElementKind::Foldout => self.foldout(parent, text, true),
            ElementKind::Dropdown => {
                let options: Vec<&str> = text.lines().collect();
                self.dropdown(parent, &options, 0)
            }
            ElementKind::Image => self.image(parent),
            ElementKind::Icon => self.icon(parent, text),
            // Empty until its owner gives it items, as a list declared in markup is.
            ElementKind::ListView => self.add(parent, Element::new(Kind::ListView, Style::column().scroll())),
            ElementKind::TreeView => self.add(parent, Element::new(Kind::TreeView, Style::column().scroll())),
            // Empty until its owner gives it a device and its keys.
            ElementKind::Keymap => self.keymap(parent),
            ElementKind::Prompt => self.prompt(parent, text),
            // Empty until its owner draws the world into it.
            ElementKind::Minimap => self.map_view(parent, crate::MapViewKind::Minimap),
            ElementKind::Map => self.map_view(parent, crate::MapViewKind::Map),
        }
    }
}
