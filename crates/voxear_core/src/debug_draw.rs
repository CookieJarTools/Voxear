//! Debug draw: lines, boxes, spheres, arrows and text in the 3D view, from
//! any code, Rust or C#, for as long as asked, as Unity's `Debug.DrawLine`.
//!
//! ```
//! use voxear_core::{debug_draw, math::Vec3};
//! debug_draw::line(Vec3::ZERO, Vec3::Y, debug_draw::GREEN, 0.0, false);
//! debug_draw::sphere(Vec3::new(0.0, 1.0, 0.0), 0.5, debug_draw::YELLOW, 2.0, true);
//! ```
//!
//! Shapes are kept here as the lines that draw them, in one list for the
//! process, so code needs no world or renderer to hand. Each lasts its
//! duration in game time: [`advance`] counts it down at the start of each
//! game frame, so a paused game keeps showing what was drawn, and a shape
//! with no duration shows for the one frame it was drawn in. The renderer
//! draws [`lines`] after post-processing, dimmed where the scene is in front
//! of them unless drawn on top, each as wide as its [`LineStyle`] asks;
//! [`texts`] are left to the UI, which has the fonts. Release builds for
//! players turn it off with [`set_enabled`], all but the lines marked as
//! part of the game.

use std::sync::{
    Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

use crate::math::{Quat, Vec3};

/// Lines kept at most; more are dropped, so a runaway loop cannot fill
/// memory or the GPU's buffer.
pub const MAX_LINES: usize = 64 * 1024;
/// Texts kept at most.
pub const MAX_TEXTS: usize = 1024;
/// Segments in each circle of a sphere.
const CIRCLE_SEGMENTS: usize = 24;

pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const RED: [f32; 4] = [1.0, 0.27, 0.27, 1.0];
pub const GREEN: [f32; 4] = [0.36, 0.9, 0.42, 1.0];
pub const BLUE: [f32; 4] = [0.33, 0.55, 1.0, 1.0];
pub const YELLOW: [f32; 4] = [1.0, 0.86, 0.3, 1.0];
pub const CYAN: [f32; 4] = [0.3, 0.9, 0.95, 1.0];
pub const MAGENTA: [f32; 4] = [0.95, 0.4, 0.95, 1.0];

/// A line to draw, its colour sRGB with alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub from: Vec3,
    pub to: Vec3,
    pub color: [f32; 4],
    /// Drawn over the scene rather than dimmed behind it.
    pub on_top: bool,
    /// Pixels wide, or 0 for the usual width.
    pub width: f32,
}

/// How a shape's lines are drawn. `false` and `true` stand for the usual
/// style, behind or on top.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineStyle {
    /// Drawn over the scene, rather than dimmed where the scene is in front.
    pub on_top: bool,
    /// Pixels wide, or 0 for the usual width.
    pub width: f32,
    /// Part of the game, kept while debug draw is off, as in a release build.
    pub in_game: bool,
}

impl From<bool> for LineStyle {
    fn from(on_top: bool) -> Self {
        Self { on_top, ..Self::default() }
    }
}

/// Text at a point in the world, its colour sRGB with alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub at: Vec3,
    pub text: String,
    pub color: [f32; 4],
}

struct Shapes {
    lines: Vec<(Line, f32)>,
    texts: Vec<(Text, f32)>,
    dropped: bool,
}

static SHAPES: Mutex<Shapes> = Mutex::new(Shapes { lines: Vec::new(), texts: Vec::new(), dropped: false });
static ENABLED: AtomicBool = AtomicBool::new(true);

fn shapes() -> MutexGuard<'static, Shapes> {
    SHAPES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Turns debug draw on or off; off, every call does nothing and what was
/// drawn is cleared. On to begin with; builds for players turn it off.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        clear();
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn line(from: Vec3, to: Vec3, color: [f32; 4], duration: f32, style: impl Into<LineStyle>) {
    add_lines([(from, to)], color, duration, style.into());
}

/// An arrow from `from` to `to`, its head a quarter of its length, at most
/// half a metre.
pub fn arrow(from: Vec3, to: Vec3, color: [f32; 4], duration: f32, style: impl Into<LineStyle>) {
    let direction = to - from;
    let length = direction.length();
    if length <= f32::EPSILON {
        return;
    }
    let forward = direction / length;
    let side = forward.any_orthonormal_vector();
    let up = forward.cross(side);
    let head = (length * 0.25).min(0.5);
    let back = to - forward * head;
    let barbs = [side, -side, up, -up].map(|d| (to, back + d * head * 0.4));
    add_lines(std::iter::once((from, to)).chain(barbs), color, duration, style.into());
}

/// A box's twelve edges, `half_size` from its centre on each axis, turned by
/// `rotation`.
pub fn wire_box(
    center: Vec3,
    half_size: Vec3,
    rotation: Quat,
    color: [f32; 4],
    duration: f32,
    style: impl Into<LineStyle>,
) {
    let corner = |i: usize| {
        let sign = Vec3::new(
            if i & 1 == 0 { -1.0 } else { 1.0 },
            if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 },
        );
        center + rotation * (half_size * sign)
    };
    // Corners that differ in one bit share an edge.
    let edges = (0..8).flat_map(|a| [1, 2, 4].map(move |bit| (a, a ^ bit))).filter(|(a, b)| a < b);
    add_lines(edges.map(|(a, b)| (corner(a), corner(b))), color, duration, style.into());
}

/// A sphere as three circles, one around each axis.
pub fn sphere(center: Vec3, radius: f32, color: [f32; 4], duration: f32, style: impl Into<LineStyle>) {
    let axes = [(Vec3::X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::Z, Vec3::X)];
    let point = |(u, v): (Vec3, Vec3), i: usize| {
        let angle = i as f32 / CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
        center + (u * angle.cos() + v * angle.sin()) * radius
    };
    let segments = axes.into_iter().flat_map(|axes| (0..CIRCLE_SEGMENTS).map(move |i| (axes, i)));
    add_lines(segments.map(|(axes, i)| (point(axes, i), point(axes, i + 1))), color, duration, style.into());
}

/// Text at a point in the world, facing the camera at a fixed size.
pub fn text(at: Vec3, text: &str, color: [f32; 4], duration: f32) {
    if !enabled() {
        return;
    }
    let mut shapes = shapes();
    if shapes.texts.len() < MAX_TEXTS {
        shapes.texts.push((Text { at, text: text.to_owned(), color }, duration.max(0.0)));
    }
}

/// Counts every shape's time down by `delta` seconds of game time, and lets
/// go of those whose time is up, the ones drawn with no duration among
/// them. The engine calls this at the start of each game frame, before
/// anything draws, and the editor each frame while not playing.
pub fn advance(delta: f32) {
    let mut shapes = shapes();
    // A shape lasts until its time is used up, but always for one frame.
    shapes.lines.retain_mut(|(_, left)| {
        *left -= delta;
        *left > 0.0
    });
    shapes.texts.retain_mut(|(_, left)| {
        *left -= delta;
        *left > 0.0
    });
}

/// The lines to draw now.
pub fn lines() -> Vec<Line> {
    shapes().lines.iter().map(|(line, _)| *line).collect()
}

/// The texts to draw now.
pub fn texts() -> Vec<Text> {
    shapes().texts.iter().map(|(text, _)| text.clone()).collect()
}

/// Lets go of everything drawn, as when play mode stops.
pub fn clear() {
    let mut shapes = shapes();
    shapes.lines.clear();
    shapes.texts.clear();
}

/// Lines from each segment's start to its end, as `style` says: what the
/// shapes above and C#'s `Draw` are made of.
pub fn segments(segments: impl IntoIterator<Item = (Vec3, Vec3)>, color: [f32; 4], duration: f32, style: LineStyle) {
    add_lines(segments, color, duration, style);
}

fn add_lines(segments: impl IntoIterator<Item = (Vec3, Vec3)>, color: [f32; 4], duration: f32, style: LineStyle) {
    if !enabled() && !style.in_game {
        return;
    }
    let mut shapes = shapes();
    let duration = duration.max(0.0);
    for (from, to) in segments {
        if shapes.lines.len() == MAX_LINES {
            if !std::mem::replace(&mut shapes.dropped, true) {
                tracing::warn!("debug draw holds {MAX_LINES} lines; more are dropped until some expire");
            }
            return;
        }
        shapes.lines.push((Line { from, to, color, on_top: style.on_top, width: style.width }, duration));
    }
    shapes.dropped = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    // One list for the process, so the tests share a lock and start clear.
    static TESTS: Mutex<()> = Mutex::new(());

    #[test]
    fn shapes_last_their_duration_in_game_time() {
        let _one = TESTS.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        line(Vec3::ZERO, Vec3::X, RED, 0.0, false);
        line(Vec3::ZERO, Vec3::Y, GREEN, 0.5, true);
        text(Vec3::ZERO, "hi", WHITE, 0.0);
        assert_eq!(lines().len(), 2, "both show the frame they were drawn");
        assert_eq!(texts().len(), 1);
        advance(0.25);
        assert_eq!(lines(), [Line { from: Vec3::ZERO, to: Vec3::Y, color: GREEN, on_top: true, width: 0.0 }]);
        assert!(texts().is_empty(), "no duration: one frame");
        advance(0.25);
        assert!(lines().is_empty());
    }

    #[test]
    fn shapes_are_their_lines() {
        let _one = TESTS.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        wire_box(Vec3::ZERO, Vec3::ONE, Quat::IDENTITY, WHITE, 0.0, false);
        let edges = lines();
        assert_eq!(edges.len(), 12);
        assert!(edges.iter().all(|l| (l.to - l.from).length() == 2.0), "each edge as long as the box");
        clear();
        sphere(Vec3::ZERO, 2.0, WHITE, 0.0, false);
        assert_eq!(lines().len(), 3 * CIRCLE_SEGMENTS);
        assert!(lines().iter().all(|l| (l.from.length() - 2.0).abs() < 1e-5));
        clear();
        arrow(Vec3::ZERO, Vec3::Z * 4.0, WHITE, 0.0, false);
        assert_eq!(lines().len(), 5, "the shaft and four barbs");
        clear();
        set_enabled(false);
        line(Vec3::ZERO, Vec3::X, RED, 1.0, false);
        let game = LineStyle { width: 3.0, in_game: true, ..LineStyle::default() };
        line(Vec3::ZERO, Vec3::Y, RED, 1.0, game);
        set_enabled(true);
        assert_eq!(lines().len(), 1, "while it is off, only what is part of the game is kept");
        assert_eq!(lines()[0].width, 3.0, "as wide as its style asks");
    }
}
