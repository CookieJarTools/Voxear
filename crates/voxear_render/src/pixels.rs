//! Drawing in pixels: the scene drawn at a few pixels to the screen's many
//! and enlarged to the view nearest, each drawn pixel a square block of
//! whole screen pixels, with no smoothing (`[post.pixels]`, see
//! `post::Pixels`). Every pass runs at the small size, a shader pack's too,
//! and TAA, FXAA and sharpening are off; the Enlarge pass
//! (`enlargeFragment` in `shaders/post.slang`) is the last.
//!
//! **Snapping** keeps the pixels from crawling as an orthographic camera
//! glides: the camera is moved onto the grid of its drawn pixels, as its
//! plane measures them from the world's middle, so each thing falls on the
//! same pixels from frame to frame, and the enlarged picture is moved by the
//! rest, a part of a drawn pixel, in whole screen pixels. The picture is
//! drawn a pixel wider on every side for that.

use voxear_core::math::{Quat, Vec3};

use crate::post::Pixels;

/// The largest drawn pixel, in screen pixels each way.
pub const MOST_PIXEL_SIZE: u32 = 32;

/// How a view is drawn in pixels this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PixelGrid {
    /// Screen pixels each drawn pixel covers, each way.
    pub size: u32,
    /// The size the scene is drawn at, margin and all.
    pub drawn: (u32, u32),
    /// Drawn pixels kept round every side for the snapped camera's rest.
    pub margin: u32,
    /// How far the enlarged picture is moved, in drawn pixels across and
    /// down: the snapped camera's rest, from -½ to ½.
    pub rest: [f32; 2],
}

impl PixelGrid {
    /// The grid for a view shown at `shown`, as `pixels` says: none when
    /// they are off. `snaps` keeps a margin for snapping.
    pub fn new(shown: (u32, u32), pixels: &Pixels, snaps: bool) -> Option<Self> {
        if !pixels.enabled {
            return None;
        }
        let size = pixels.size_for(shown.1);
        let margin = u32::from(snaps);
        let across = |shown: u32| shown.div_ceil(size).max(1) + 2 * margin;
        Some(Self { size, drawn: (across(shown.0), across(shown.1)), margin, rest: [0.0; 2] })
    }

    /// How many times as far each way, across and up, the drawn picture sees
    /// as the view would: what the camera's lens is grown by.
    pub fn growth(&self, shown: (u32, u32)) -> [f32; 2] {
        let seen = |drawn: u32, shown: u32| drawn as f32 * self.size as f32 / shown.max(1) as f32;
        [seen(self.drawn.0, shown.0), seen(self.drawn.1, shown.1)]
    }

    /// Where the view's first screen pixel falls in the drawn picture, in
    /// its pixels: both share their middle, moved by the rest.
    pub fn start(&self, shown: (u32, u32)) -> [f32; 2] {
        let start =
            |drawn: u32, shown: u32, rest: f32| drawn as f32 * 0.5 - shown as f32 / (2.0 * self.size as f32) + rest;
        [start(self.drawn.0, shown.0, self.rest[0]), start(self.drawn.1, shown.1, self.rest[1])]
    }

    /// The camera at `position`, turned by `rotation`, moved onto the grid
    /// of drawn pixels `metres` wide in its plane, nearest; this grid then
    /// moves the enlarged picture by the rest.
    pub fn snap(&mut self, position: Vec3, rotation: Quat, metres: f32) -> Vec3 {
        if self.margin == 0 || metres <= 0.0 {
            return position;
        }
        let (right, up) = (rotation * Vec3::X, rotation * Vec3::Y);
        let (across, height) = (position.dot(right) / metres, position.dot(up) / metres);
        let (snapped_across, snapped_height) = (across.round(), height.round());
        // The camera further right shows the world further left, so the picture is read from further right; further
        // up, the picture is read from higher, its rows counting down.
        self.rest = [across - snapped_across, snapped_height - height];
        position + right * (snapped_across - across) * metres + up * (snapped_height - height) * metres
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(size: u32, height: u32) -> Pixels {
        Pixels { enabled: true, size, height, snap: true }
    }

    #[test]
    fn off_draws_no_grid() {
        assert_eq!(PixelGrid::new((1920, 1080), &Pixels { enabled: false, ..pixels(4, 0) }, true), None);
    }

    #[test]
    fn the_drawn_picture_is_the_view_over_the_pixel_size_and_a_margin() {
        let grid = PixelGrid::new((1920, 1080), &pixels(4, 0), false).unwrap();
        assert_eq!((grid.drawn, grid.margin), ((480, 270), 0));
        assert_eq!(grid.start((1920, 1080)), [0.0, 0.0]);
        let grid = PixelGrid::new((1921, 1080), &pixels(4, 0), true).unwrap();
        assert_eq!((grid.drawn, grid.margin), ((483, 272), 1), "a part pixel rounded up, and one each side");
        let start = grid.start((1921, 1080));
        assert!((start[1] - 1.0).abs() < 1e-6, "the view starts a margin in: {start:?}");
        let growth = grid.growth((1921, 1080));
        assert!((growth[1] - 272.0 * 4.0 / 1080.0).abs() < 1e-6);
    }

    #[test]
    fn a_height_picks_the_nearest_whole_pixel_size() {
        assert_eq!(PixelGrid::new((1920, 1080), &pixels(4, 180), false).unwrap().size, 6);
        assert_eq!(PixelGrid::new((1920, 1080), &pixels(4, 200), false).unwrap().size, 5);
        assert_eq!(PixelGrid::new((640, 360), &pixels(4, 2000), false).unwrap().size, 1, "never under a pixel");
    }

    #[test]
    fn snapping_moves_the_camera_onto_the_grid_and_the_picture_by_the_rest() {
        let mut grid = PixelGrid::new((800, 600), &pixels(4, 0), true).unwrap();
        let rotation = Quat::from_rotation_x(0.6);
        let (right, up) = (rotation * Vec3::X, rotation * Vec3::Y);
        let at = right * 10.3 * 0.05 + up * 7.8 * 0.05 + rotation * Vec3::Z * 3.0;
        let snapped = grid.snap(at, rotation, 0.05);
        let across = snapped.dot(right) / 0.05;
        let height = snapped.dot(up) / 0.05;
        assert!((across - 10.0).abs() < 1e-3 && (height - 8.0).abs() < 1e-3, "on the grid: {across}, {height}");
        assert!((snapped - at).dot(rotation * Vec3::Z).abs() < 1e-5, "not moved along the view");
        assert!((grid.rest[0] - 0.3).abs() < 1e-3 && (grid.rest[1] - 0.2).abs() < 1e-3, "{:?}", grid.rest);
    }

    #[test]
    fn a_gliding_camera_moves_the_picture_evenly_without_crawling() {
        // The camera glides right a tenth of a drawn pixel a frame: what is drawn moves a whole drawn pixel at a time,
        // and the enlarged picture's start, where the screen reads it from, moves on evenly between.
        let shown = (800, 600);
        let mut last: Option<f32> = None;
        for frame in 0..40 {
            let mut grid = PixelGrid::new(shown, &pixels(4, 0), true).unwrap();
            let at = Vec3::new(frame as f32 * 0.1 * 0.05, 0.0, 0.0);
            let snapped = grid.snap(at, Quat::IDENTITY, 0.05);
            // Where a fixed point of the world lands on screen, in screen pixels: its drawn pixel from the snapped
            // camera, less the start.
            let drawn = grid.drawn.0 as f32 * 0.5 + (0.0 - snapped.x) / 0.05;
            let screen = (drawn - grid.start(shown)[0]) * grid.size as f32;
            if let Some(last) = last {
                let moved = last - screen;
                assert!((moved - 0.4).abs() < 1e-2, "a tenth of a 4-pixel block each frame at frame {frame}: {moved}");
            }
            last = Some(screen);
        }
    }
}
