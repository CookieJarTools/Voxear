//! Voxear UI: a retained tree of elements, laid out with flexbox and drawn
//! as rounded, bordered rectangles and crisp text.
//!
//! - **Layout** is taffy's flexbox, so it behaves like CSS and Unity's UI
//!   Toolkit.
//! - **Text** is shaped with parley and rasterised with swash, hinted at the
//!   exact pixel size on screen into a glyph atlas, and placed on whole
//!   pixels, so it stays crisp at any display scale.
//! - **Drawing** produces a [`UiFrame`]: a list of [`Quad`]s and, when new
//!   glyphs appeared, the atlas to upload. The renderer draws it over the
//!   game; nothing here touches the GPU, so the tree can be built and laid
//!   out in tests.
//!
//! - **Markup** (`.vui`, see [`markup`]) and **style sheets** (`.vss`, see
//!   [`StyleSheet`]) describe UI as Unity's UXML and USS do, with CSS's
//!   cascade, inherited text and custom properties for theme tokens.
//! - **The pointer** drives `:hover` and `:active`, clicks and the wheel.
//! - **Controls**: scroll views, list and tree views that keep elements
//!   only for the rows on screen (see [`ListItems`] and [`TreeItems`]),
//!   text fields, toggles, sliders and foldouts, with keyboard focus, and
//!   split views, tabs and dropdowns, and docking: areas of tabbed panels
//!   rearranged by dragging tabs.
//! - **Context menus** ([`MenuItem`]), opened on the secondary button's
//!   clicks, and **dragging**: rows onto one another in lists and trees, and
//!   rows and elements onto elements that take drops ([`DragSource`]).
//! - **Rich text** ([`RichText`]): labels with bold, italic, code,
//!   struck-out and highlighted words, and links to click.
//! - **Transitions** ease colours and opacity between styles.
//! - **A default theme** in the editor's look, which sheets can override,
//!   and [`Theme`]s made from a base, an accent and a contrast.
//!
//! This is the foundation the editor and in-game UI build on.

mod animation;
mod atlas;
mod cascade;
mod color;
mod color_picker;
mod controls;
mod create;
mod dock;
mod drag;
mod draw;
mod icons;
mod input;
mod list;
pub mod markup;
mod menu;
mod panes;
mod pointer;
pub mod properties;
mod rich;
mod scroll;
mod selector;
mod sheet;
mod style;
mod theme;
mod tree;

pub use color::Color;
pub use color_picker::{ColorPicker, PickedColor, hsv_to_srgb, srgb_to_hsv};
pub use create::ElementKind;
pub use drag::DragSource;
pub use draw::Shape;
pub use icons::NAMES as ICONS;
pub use input::{Clipboard, Key, LocalClipboard};
pub use list::{DropPlace, ItemFns, ListItems, TreeItems};
pub use markup::LoadFile;
pub use menu::MenuItem;
pub use pointer::Modifiers;
pub use rich::{Marks, RichText, Span};
pub use sheet::StyleSheet;
pub use style::{
    Animated, Easing, Family, KeyframeAnimation, Overflow, Shadow, Style, TextAlign, TextStyle, Transition,
};
pub use theme::Theme;
pub use tree::{Gradient, ImageRef};
pub use tree::{Kind, NodeId, Ui};

/// The default theme every [`Ui`] starts with: `theme/Default.vss`.
pub const DEFAULT_THEME: &str = include_str!("../theme/Default.vss");

/// A document that could not be loaded, with where and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiError(pub String);

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UiError {}

/// One thing to draw: a rounded, bordered rectangle, a glyph or icon from
/// the atlas, a soft shadow or a line. Coordinates are physical pixels from
/// the top left; colours are linear and premultiplied. The layout matches
/// `UiQuad` in the renderer's shaders.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    pub rect: [f32; 4],
    /// The glyph's place in the atlas, as fractions of its size.
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    /// Left, top, right and bottom of the area it may draw in, such as the
    /// inside of a scroll view. The GPU trims the quad to it.
    pub clip: [f32; 4],
    pub radius: f32,
    pub border: f32,
    /// [`Quad::SHAPE`], [`Quad::GLYPH`], [`Quad::SHADOW`], [`Quad::LINE`],
    /// [`Quad::IMAGE`] or [`Quad::GRADIENT`].
    pub kind: u32,
    /// For shapes, [`Quad::DASHED`] and [`Quad::BACKDROP`]; for an image,
    /// its texture; for a gradient, [`Quad::FADES_DOWN`].
    pub flags: u32,
}

impl Quad {
    pub const SHAPE: u32 = 0;
    pub const GLYPH: u32 = 1;
    /// A blurred rounded box: `uv` is the box (left, top, width and height),
    /// `border` the blur's standard deviation, and `border_color` the
    /// element's own box (the same way round), which the shadow is cut out
    /// of, as CSS's are.
    pub const SHADOW: u32 = 2;
    /// A line with round ends from `uv`'s first point to its second,
    /// `border` pixels wide.
    pub const LINE: u32 = 3;
    /// Part of a texture, `uv` of it, in a box rounded by `radius`: `flags`
    /// is the renderer's texture, and `color` tints it.
    pub const IMAGE: u32 = 4;
    /// A box rounded by `radius` filled from `color` at the left to
    /// `border_color` at the right, and with [`Quad::FADES_DOWN`], down to
    /// `uv` at the bottom. These colours are sRGB with straight alpha, and
    /// blend as people see them.
    pub const GRADIENT: u32 = 5;
    /// For a gradient: it fades down to `uv`.
    pub const FADES_DOWN: u32 = 1;
    /// The border is drawn in dashes.
    pub const DASHED: u32 = 1;
    /// For a shape: what is behind it shows through its background, blurred,
    /// as CSS's `backdrop-filter: blur()` makes frosted glass.
    pub const BACKDROP: u32 = 2;
    /// A clip that clips nothing.
    pub const UNCLIPPED: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];
}

impl Default for Quad {
    fn default() -> Self {
        Self {
            rect: [0.0; 4],
            uv: [0.0; 4],
            color: [0.0; 4],
            border_color: [0.0; 4],
            clip: Self::UNCLIPPED,
            radius: 0.0,
            border: 0.0,
            kind: Self::SHAPE,
            flags: 0,
        }
    }
}

/// The glyph atlas: one byte of coverage per pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Goes up whenever the pixels change, so the renderer uploads only new
    /// versions, however many frames it skipped.
    pub version: u64,
}

/// Everything the renderer needs to draw the UI for one frame.
#[derive(Clone, Debug, Default)]
pub struct UiFrame {
    pub quads: Vec<Quad>,
    /// The atlas as the glyphs in `quads` need it. Shared, so a frame costs
    /// no copy of it; `None` when no text was ever drawn.
    pub atlas: Option<std::sync::Arc<AtlasImage>>,
}
