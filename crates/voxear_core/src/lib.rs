//! Foundations shared by every Voxear crate.

pub mod logging;
pub mod time;

pub use glam as math;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
