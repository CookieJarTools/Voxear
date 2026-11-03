//! A prompt: the controls that perform an action, each drawn as a glyph,
//! as games show "press A to jump". Its owner gives it the glyphs (see
//! `voxear_engine::keymaps::prompt_glyphs`), and it makes them again only
//! when they change, as when the player picks up a gamepad.
//!
//! Each glyph is a `prompt-glyph` element with its shape's class, `key`,
//! `round`, `pill`, `square`, `stick` or `bare`, holding its text in a
//! `prompt-text` label or its `prompt-icon`, coloured as the device colours it, so
//! sheets can restyle every glyph at once.

use crate::{Color, NodeId, Style, Ui};

/// The outline a glyph is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphShape {
    #[default]
    Key,
    Round,
    Pill,
    Square,
    Stick,
    Bare,
}

impl GlyphShape {
    fn class(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Round => "round",
            Self::Pill => "pill",
            Self::Square => "square",
            Self::Stick => "stick",
            Self::Bare => "bare",
        }
    }
}

/// One control in a prompt: its outline, its text or one of
/// [`crate::ICONS`], and the colour they are drawn in, if the device has one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptGlyph {
    pub shape: GlyphShape,
    pub text: String,
    pub icon: Option<&'static str>,
    pub color: Option<Color>,
}

/// A prompt's action, by name, and the glyphs it shows.
#[derive(Clone, Debug, Default)]
pub(crate) struct Prompt {
    pub action: String,
    pub glyphs: Vec<PromptGlyph>,
}

impl Ui {
    /// A prompt for the action named `action`, such as `Jump`, empty until
    /// its owner gives it glyphs.
    pub fn prompt(&mut self, parent: NodeId, action: &str) -> NodeId {
        let node = self.panel(parent, Style::row());
        self.add_class(node, "prompt");
        self.prompts.insert(node.0, Prompt { action: action.to_string(), glyphs: Vec::new() });
        node
    }

    pub fn is_prompt(&self, node: NodeId) -> bool {
        self.prompts.contains_key(&node.0)
    }

    /// Every prompt, with the action it is for.
    pub fn prompts(&self) -> Vec<(NodeId, String)> {
        self.prompts.iter().map(|(&node, prompt)| (NodeId(node), prompt.action.clone())).collect()
    }

    pub fn prompt_action(&self, node: NodeId) -> Option<&str> {
        self.prompts.get(&node.0).map(|prompt| prompt.action.as_str())
    }

    /// Makes a prompt for another action; its glyphs follow when its owner
    /// gives them.
    pub fn set_prompt_action(&mut self, node: NodeId, action: &str) {
        if let Some(prompt) = self.prompts.get_mut(&node.0) {
            prompt.action = action.to_string();
        }
    }

    /// Shows a prompt's glyphs, making them again only when they changed.
    pub fn set_prompt(&mut self, node: NodeId, glyphs: Vec<PromptGlyph>) {
        let Some(prompt) = self.prompts.get(&node.0) else { return };
        if prompt.glyphs == glyphs {
            return;
        }
        for child in self.children(node) {
            self.remove(child);
        }
        for glyph in &glyphs {
            self.add_glyph(node, glyph);
        }
        if let Some(prompt) = self.prompts.get_mut(&node.0) {
            prompt.glyphs = glyphs;
        }
    }

    fn add_glyph(&mut self, prompt: NodeId, glyph: &PromptGlyph) {
        let node = self.panel(prompt, Style::row());
        self.add_class(node, "prompt-glyph");
        self.add_class(node, glyph.shape.class());
        let inner = match glyph.icon {
            Some(icon) => {
                let shown = self.icon(node, icon);
                self.add_class(shown, "prompt-icon");
                shown
            }
            None => {
                let label = self.add_label(node, &glyph.text);
                self.add_class(label, "prompt-text");
                label
            }
        };
        if let Some(color) = glyph.color {
            self.set_inline_style(inner, &format!("color: {}", color.css()));
        }
    }

    pub(crate) fn forget_prompt(&mut self, node: taffy::NodeId) {
        self.prompts.remove(&node);
    }
}
