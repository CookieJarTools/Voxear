//! The VoxDocs: a project's own documentation, kept as Markdown pages in
//! its `Docs` folder and read and written in the editor, after the
//! CookieJarTools Cookbook for Unity.
//!
//! - [`markdown`] reads the VoxDocs Markdown into blocks of rich text.
//! - [`CodeReference`] is the code's own pages, made from its comments.
//! - [`Library`] is every page and folder in `Docs`, found by id, path or
//!   title, and changed and saved.
//! - [`render`](view::render) draws a page as Voxear UI elements.
//! - [`Health`](health::Health) is what needs looking after in the pages.
//! - [`VoxDocs`] is the panel: the pages down the side, the page beside
//!   them, links, checklists, search, back and forward, and writing.

pub mod code;
pub mod header;
pub mod health;
mod library;
pub mod markdown;
mod panel;
mod progress;
pub mod view;

pub use code::CodeReference;
pub use header::Header;
pub use library::{Excerpt, Library, Page, PageId, Target};
pub use panel::VoxDocs;
