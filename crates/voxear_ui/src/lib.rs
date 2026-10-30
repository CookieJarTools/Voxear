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
//! - **Controls**: scroll views, and list and tree views that keep elements
//!   only for the rows on screen (see [`ListItems`] and [`TreeItems`]).
//!
//! This is the foundation the editor and in-game UI build on.

mod atlas;
mod cascade;
mod color;
mod draw;
mod list;
pub mod markup;
mod pointer;
pub mod properties;
mod scroll;
mod selector;
mod sheet;
mod style;
mod tree;

pub use color::Color;
pub use list::{ItemFns, ListItems, TreeItems};
pub use markup::LoadFile;
pub use pointer::Modifiers;
pub use sheet::StyleSheet;
pub use style::{Family, Overflow, Style, TextStyle};
pub use tree::{Kind, NodeId, Ui};

/// A document that could not be loaded, with where and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiError(pub String);

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UiError {}

/// One thing to draw: a rounded, bordered rectangle, or a glyph from the
/// atlas. Coordinates are physical pixels from the top left; colours are
/// linear and premultiplied. The layout matches `UiQuad` in the renderer's
/// shaders.
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
    /// [`Quad::SHAPE`] or [`Quad::GLYPH`].
    pub kind: u32,
    pub _pad: u32,
}

impl Quad {
    pub const SHAPE: u32 = 0;
    pub const GLYPH: u32 = 1;
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
            _pad: 0,
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
