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
//! - **The pointer** drives `:hover` and `:active`, and clicks.
//!
//! This is the foundation the editor and in-game UI build on.

mod atlas;
mod cascade;
mod color;
mod draw;
pub mod markup;
mod pointer;
pub mod properties;
mod selector;
mod sheet;
mod style;
mod tree;

pub use color::Color;
pub use markup::LoadFile;
pub use sheet::StyleSheet;
pub use style::{Family, Style, TextStyle};
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
/// linear and premultiplied. The layout matches `Quad` in `ui.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    pub rect: [f32; 4],
    /// The glyph's place in the atlas, as fractions of its size.
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub radius: f32,
    pub border: f32,
    /// [`Quad::SHAPE`] or [`Quad::GLYPH`].
    pub kind: u32,
    pub _pad: u32,
}

impl Quad {
    pub const SHAPE: u32 = 0;
    pub const GLYPH: u32 = 1;
}

/// The glyph atlas: one byte of coverage per pixel.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Everything the renderer needs to draw the UI for one frame.
#[derive(Clone, Debug, Default)]
pub struct UiFrame {
    pub quads: Vec<Quad>,
    /// Set when the atlas changed since the last frame, so the GPU copy is
    /// refreshed.
    pub atlas: Option<AtlasImage>,
}
