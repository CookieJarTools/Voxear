//! Foundations shared by every Voxear crate.

pub mod debug_draw;
pub mod logging;
pub mod memory;
pub mod profiling;
pub mod time;

pub use glam as math;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
