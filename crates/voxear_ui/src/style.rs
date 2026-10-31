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
    /// The border is drawn in dashes rather than a solid line.
    pub dashed: bool,
    pub radius: f32,
    /// A soft shadow drawn under it, outside its edge.
    pub shadow: Option<Shadow>,
    /// Multiplies this element's and its children's alpha.
    pub opacity: f32,
    /// Whether the pointer can hover and press it. Its children decide for
    /// themselves.
    pub pickable: bool,
    /// What happens to children that do not fit.
    pub overflow: Overflow,
    /// The scrollbar thumb's colour, for elements that scroll.
    pub scrollbar: Color,
    /// Behind selected text in a text field.
    pub selection: Color,
    /// A text field's caret; its text colour when not set.
    pub caret: Option<Color>,
    /// Which changes animate rather than jump, and how.
    pub transitions: Vec<Transition>,
}

/// A blurred copy of an element's box, drawn under it, as CSS's
/// `box-shadow` is. Lengths are logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    /// How far the edge blurs: CSS's blur radius, twice the Gaussian's
    /// standard deviation.
    pub blur: f32,
    /// Grows the box before it is blurred.
    pub spread: f32,
    pub color: Color,
}

/// A property that eases from its old value to its new one when the cascade
/// changes it, as CSS's `transition` does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub property: Animated,
    /// In seconds.
    pub duration: f32,
    pub delay: f32,
    pub easing: Easing,
}

/// The properties that can transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Animated {
    /// Every one below.
    All,
    Background,
    BorderColor,
    Color,
    Opacity,
}

/// How a transition moves from start to end over its time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Easing {
    Linear,
    /// CSS's default: a quick start that settles.
    #[default]
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Progress at time `t`, both from 0 to 1, along CSS's cubic Béziers.
    pub fn at(self, t: f32) -> f32 {
        if t <= 0.0 || t >= 1.0 {
            return t.clamp(0.0, 1.0);
        }
        let (x1, y1, x2, y2) = match self {
            Easing::Linear => return t,
            Easing::Ease => (0.25, 0.1, 0.25, 1.0),
            Easing::EaseIn => (0.42, 0.0, 1.0, 1.0),
            Easing::EaseOut => (0.0, 0.0, 0.58, 1.0),
            Easing::EaseInOut => (0.42, 0.0, 0.58, 1.0),
        };
        let bezier = |a: f32, b: f32, s: f32| 3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3);
        // Solve x(s) = t by bisection, which is plenty for a UI.
        let (mut low, mut high) = (0.0f32, 1.0f32);
        for _ in 0..20 {
            let mid = (low + high) / 2.0;
            if bezier(x1, x2, mid) < t { low = mid } else { high = mid }
        }
        bezier(y1, y2, (low + high) / 2.0)
    }
}

/// What an element does with children that do not fit inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    /// They show outside it.
    #[default]
    Visible,
    /// They are cut off at its padding edge.
    Hidden,
    /// They are cut off, and the wheel or the scrollbar moves them.
    Scroll,
}

impl Default for Style {
    /// Every element is positioned (`position: relative`), so an absolute
    /// child is placed in its parent, as in Unity's UI Toolkit, rather than
    /// in the nearest positioned ancestor as CSS's `static` default would.
    fn default() -> Self {
        Self {
            layout: taffy::Style { position: Position::Relative, ..taffy::Style::default() },
            background: None,
            border: None,
            dashed: false,
            radius: 0.0,
            shadow: None,
            opacity: 1.0,
            pickable: true,
            overflow: Overflow::Visible,
            scrollbar: Color::WHITE.with_alpha(0x40),
            selection: Color::hex(0x8b93ff).with_alpha(0x66),
            caret: None,
            transitions: Vec::new(),
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

    /// Scrolls its children when they do not fit; see [`Overflow::Scroll`].
    pub fn scroll(mut self) -> Self {
        self.overflow = Overflow::Scroll;
        self
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
    pub align: TextAlign,
    /// The height of each line in logical pixels, or 0 for the font's own.
    pub line_height: f32,
}

/// Where lines of text sit across their element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Left, or right in right-to-left text.
    #[default]
    Start,
    Center,
    End,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size: 13.0,
            weight: 400.0,
            color: Color::WHITE,
            family: Family::Sans,
            align: TextAlign::Start,
            line_height: 0.0,
        }
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

    pub(crate) fn parley_line_height(self) -> parley::LineHeight {
        if self.line_height > 0.0 {
            parley::LineHeight::Absolute(self.line_height)
        } else {
            parley::LineHeight::default()
        }
    }
}
