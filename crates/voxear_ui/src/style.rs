use taffy::{
    AlignItems, Dimension, FlexDirection, JustifyContent, LengthPercentage, LengthPercentageAuto, Position, Rect, Size,
};

use crate::Color;

/// How an element looks and where it goes. Layout fields are taffy's
/// (flexbox, as in CSS); the builder methods cover the common ones. Style
/// sheets set the same fields by their CSS names.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub layout: taffy::Style,
    pub background: Option<Color>,
    /// Width in logical pixels, and colour.
    pub border: Option<(f32, Color)>,
    pub radius: f32,
    /// Multiplies this element's and its children's alpha.
    pub opacity: f32,
    /// Whether the pointer can hover and press it. Its children decide for
    /// themselves.
    pub pickable: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            layout: taffy::Style::default(),
            background: None,
            border: None,
            radius: 0.0,
            opacity: 1.0,
            pickable: true,
        }
    }
}

impl Style {
    /// Children stacked top to bottom.
    pub fn column() -> Self {
        let mut style = Self::default();
        style.layout.flex_direction = FlexDirection::Column;
        style
    }

    /// Covers its parent and lets the pointer through to what is under it,
    /// as a document's container does.
    pub fn cover() -> Self {
        let mut style = Self::column();
        style.layout.position = Position::Absolute;
        style.layout.inset = Rect::length(0.0);
        style.pickable = false;
        style
    }

    /// Children side by side.
    pub fn row() -> Self {
        let mut style = Self::default();
        style.layout.flex_direction = FlexDirection::Row;
        style.layout.align_items = AlignItems::CENTER;
        style
    }

    pub fn padding(mut self, px: f32) -> Self {
        self.layout.padding = Rect::length(px);
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.layout.gap = Size { width: LengthPercentage::length(px), height: LengthPercentage::length(px) };
        self
    }

    pub fn width(mut self, px: f32) -> Self {
        self.layout.size.width = Dimension::length(px);
        self
    }

    pub fn height(mut self, px: f32) -> Self {
        self.layout.size.height = Dimension::length(px);
        self
    }

    /// Takes the space left over along the parent's direction.
    pub fn grow(mut self) -> Self {
        self.layout.flex_grow = 1.0;
        self
    }

    /// Pushes the children apart, to opposite ends.
    pub fn spread(mut self) -> Self {
        self.layout.justify_content = JustifyContent::SPACE_BETWEEN;
        self
    }

    /// Placed at a fixed offset from the parent's top left, out of the flow.
    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.layout.position = Position::Absolute;
        self.layout.inset.left = LengthPercentageAuto::length(x);
        self.layout.inset.top = LengthPercentageAuto::length(y);
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// A border, which also insets the content, as in CSS.
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Some((width, color));
        self.layout.border = Rect::length(width);
        self
    }

    pub fn radius(mut self, px: f32) -> Self {
        self.radius = px;
        self
    }
}

/// Which typeface: the UI's sans-serif, or monospaced for numbers and code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Family {
    #[default]
    Sans,
    Mono,
}

impl Family {
    /// What each asks the system for, best first. Inter, the editor's face,
    /// is used whenever it is installed.
    pub(crate) fn source(self) -> &'static str {
        match self {
            Family::Sans => "Inter, \"Noto Sans\", \"DejaVu Sans\", sans-serif",
            Family::Mono => "\"JetBrains Mono\", \"Noto Sans Mono\", \"DejaVu Sans Mono\", monospace",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// In logical pixels.
    pub size: f32,
    /// 400 is regular, 600 semibold.
    pub weight: f32,
    pub color: Color,
    pub family: Family,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self { size: 13.0, weight: 400.0, color: Color::WHITE, family: Family::Sans }
    }
}

impl TextStyle {
    pub fn size(self, size: f32) -> Self {
        Self { size, ..self }
    }

    pub fn weight(self, weight: f32) -> Self {
        Self { weight, ..self }
    }

    pub fn color(self, color: Color) -> Self {
        Self { color, ..self }
    }

    pub fn mono(self) -> Self {
        Self { family: Family::Mono, ..self }
    }
}
