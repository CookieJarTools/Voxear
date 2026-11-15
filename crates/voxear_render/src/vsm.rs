//! Virtual shadow maps for the sun.
//!
//! The sun's shadows are drawn into a clipmap of `LEVELS` virtual shadow
//! maps round the camera, each `WINDOW` pages of `PAGE` texels a side, the
//! finest `FINEST_TEXEL` metres a texel and each level's texels twice the
//! last's, so the coarsest reaches about four kilometres either way. Only
//! the pages the view needs are drawn, into slots of a pool texture, and
//! kept from frame to frame for as long as they are needed and nothing
//! drawn in them changes.
//!
//! Each frame the view's depth asks for pages: each pixel the page of the
//! level whose texels are no larger than the pixel is there, or the first
//! level whose window reaches it, and the two coarser, which soft shadows
//! read (`markMain` in
//! `shaders/virtual_shadows.slang`). The CPU reads what a frame asked for
//! once its frame in flight comes round again, gives each page asked for a
//! slot of the pool, the coarsest first, and draws those newly given and
//! those gone stale, up to `PAGES_A_FRAME` a frame; the rest wait, and
//! shading reads the next coarser page that is drawn meanwhile.
//!
//! A page in the pool holds what stands still (`INSTANCE_MOVING` left out):
//! it goes stale only when something in it comes, goes, stops or starts
//! moving, a chunk of blocks or a terrain changes, or a material is
//! written. Each frame, each page something moving stands in is copied into
//! a slot of a smaller moving pool, and what moves drawn over the copy;
//! shading reads that copy where the page table says so. A few
//! coarse pages round the camera are always kept, for what the view cannot
//! ask for: the fog's light shafts and the bounced light's rays.
//!
//! A page's casters are drawn into its slot with `BORDER` texels of border
//! all round, so every tap of a filter centred in the page, which reaches
//! no further than that, reads its own slot. Pages are placed
//! by where they are in the sun's own space, from the world's middle, so a
//! page keeps its place as the camera moves, and the sun keeps its pages
//! until it turns by more than `TURN_DEGREES`.

use std::collections::{HashMap, HashSet};

use voxear_core::math::Vec3;
use voxear_rhi::FRAMES_IN_FLIGHT;

/// The clipmap's levels, finest first.
pub const LEVELS: usize = 10;
/// Texels a side of a page.
pub const PAGE: u32 = 128;
/// Texels of border all round a page in its slot, drawn as the page's
/// own, and texels a side of a page's slot in the pool.
pub const BORDER: u32 = 8;
pub const SLOT: u32 = PAGE + 2 * BORDER;
/// Slots a side of the pool, and the pool's texels a side.
pub const POOL_SLOTS: u32 = 32;
pub const POOL: u32 = SLOT * POOL_SLOTS;
const SLOTS: usize = (POOL_SLOTS * POOL_SLOTS) as usize;
/// Pages a side of each level round the camera.
pub const WINDOW: i32 = 64;
const WINDOW_PAGES: usize = (WINDOW * WINDOW) as usize;
/// A texel of the finest level, in metres.
pub const FINEST_TEXEL: f32 = 1.0 / 512.0;
/// How deep the sun's view reaches along its light, round the camera, in
/// metres, and the steps its middle moves in.
pub const DEPTH_RANGE: f32 = 8000.0;
const DEPTH_STEP: f32 = 1000.0;
/// The most pages drawn in a frame.
pub const PAGES_A_FRAME: usize = 96;
/// Slots a side of the moving pool, which holds this frame's copies of the
/// pages what moves stands in, with it drawn over them, and how many.
pub const MOVING_POOL_SLOTS: u32 = 16;
pub const MOVING_POOL: u32 = SLOT * MOVING_POOL_SLOTS;
pub const MOVING_SLOTS: usize = (MOVING_POOL_SLOTS * MOVING_POOL_SLOTS) as usize;
/// The words of a frame's requests: a bit for each page of each level's
/// window.
pub const REQUEST_WORDS: usize = LEVELS * WINDOW_PAGES / 32;
/// How far the sun turns before every page is drawn again, in degrees.
pub const TURN_DEGREES: f32 = 0.05;
/// The coarse pages always kept round the camera: a level and how far
/// round, in metres.
const KEPT_ROUND: [(usize, f32); 2] = [(7, 128.0), (9, 512.0)];

/// How wide a page of `level` is, in metres.
pub fn page_metres(level: usize) -> f32 {
    FINEST_TEXEL * PAGE as f32 * (1u32 << level) as f32
}

/// The sun's own space: across, up and along its light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basis {
    pub right: Vec3,
    pub up: Vec3,
    pub direction: Vec3,
}

impl Basis {
    /// The space of light travelling along `direction`.
    pub fn of(direction: Vec3) -> Self {
        let direction = direction.normalize_or(Vec3::NEG_Y);
        let reference = if direction.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let right = reference.cross(direction).normalize();
        let up = direction.cross(right);
        Self { right, up, direction }
    }

    /// Where `point` is across and up the sun's view.
    pub fn across(&self, point: Vec3) -> [f32; 2] {
        [self.right.dot(point), self.up.dot(point)]
    }
}

/// A page of a level, by where it is in the sun's space: its column and
/// row, from the world's middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageKey {
    pub level: u8,
    pub x: i32,
    pub y: i32,
}

/// A page held in a slot of the pool: whether it has been drawn there, and
/// whether something drawn in it has changed since, and the last frame it
/// was asked for.
#[derive(Clone, Copy, Debug)]
struct Page {
    slot: u16,
    drawn: bool,
    stale: bool,
    used: u64,
}

/// What a frame's virtual shadows draw and read, for the GPU (see
/// `GpuVirtualShadows`).
#[derive(Clone, Debug, Default)]
pub struct VsmFrame {
    pub basis: Option<Basis>,
    pub depth_min: f32,
    /// Each level's window's first page, column and row.
    pub origins: [[i32; 2]; LEVELS],
    /// Each page of each level's window, row after row: its slot plus one,
    /// or 0 where none is drawn.
    pub table: Vec<u32>,
    /// The pages drawn this frame, by level, coarsest last: each page and
    /// its slot.
    pub drawn: Vec<(PageKey, u16)>,
    /// The pages what moves stands in this frame, by level: each page, its
    /// slot, and its slot in the moving pool.
    pub moving: Vec<(PageKey, u16, u16)>,
    /// How many pages are held in the pool.
    pub held: usize,
}

impl VsmFrame {
    /// The levels with pages drawn this frame, in order.
    pub fn levels(&self) -> Vec<usize> {
        let mut levels: Vec<usize> = self.drawn.iter().map(|(key, _)| key.level as usize).collect();
        levels.dedup();
        levels
    }

    /// Of the pages drawn this frame, the box in the sun's space round those
    /// of `level`, with their borders, as low and high corners.
    pub fn level_box(&self, level: usize) -> Option<([f32; 2], [f32; 2])> {
        box_round(self.drawn.iter().map(|(key, _)| key), level)
    }

    /// The same round the pages of `level` what moves stands in.
    pub fn moving_box(&self, level: usize) -> Option<([f32; 2], [f32; 2])> {
        box_round(self.moving.iter().map(|(key, ..)| key), level)
    }
}

/// The box in the sun's space round the pages of `level` among `keys`, with
/// their borders, as low and high corners.
fn box_round<'a>(keys: impl Iterator<Item = &'a PageKey>, level: usize) -> Option<([f32; 2], [f32; 2])> {
    let metres = page_metres(level);
    let border = metres / PAGE as f32 * BORDER as f32;
    let mut keys = keys.filter(|key| key.level as usize == level).peekable();
    keys.peek()?;
    let (mut low, mut high) = ([i32::MAX; 2], [i32::MIN; 2]);
    for key in keys {
        low = [low[0].min(key.x), low[1].min(key.y)];
        high = [high[0].max(key.x + 1), high[1].max(key.y + 1)];
    }
    let corner = |page: [i32; 2], by: f32| [page[0] as f32 * metres + by, page[1] as f32 * metres + by];
    Some((corner(low, -border), corner(high, border)))
}

/// What the shaders read of a frame's virtual shadows: its pages, the page
/// table's and requests' buffers, and the pool's texture.
pub(crate) fn gpu(
    frame: &VsmFrame,
    (table, requests): (u64, u64),
    (pool, moving_pool): (u32, u32),
) -> crate::gpu::GpuVirtualShadows {
    let basis = frame.basis.unwrap_or_else(|| Basis::of(Vec3::NEG_Y));
    let mut gpu = crate::gpu::GpuVirtualShadows {
        right: basis.right.to_array(),
        finest_texel: FINEST_TEXEL,
        up: basis.up.to_array(),
        depth_min: frame.depth_min,
        direction: basis.direction.to_array(),
        depth_range: DEPTH_RANGE,
        table,
        requests,
        pool,
        moving_pool,
        drawn: frame.drawn.len() as u32,
        ..crate::gpu::GpuVirtualShadows::default()
    };
    let planes_round = |found: Option<([f32; 2], [f32; 2])>| match found {
        Some((low, high)) => {
            let (right, up) = (basis.right, basis.up);
            [
                [right.x, right.y, right.z, -low[0]],
                [-right.x, -right.y, -right.z, high[0]],
                [up.x, up.y, up.z, -low[1]],
                [-up.x, -up.y, -up.z, high[1]],
                [0.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        }
        // A level with no page culls everything.
        None => [[0.0, 0.0, 0.0, -1.0e9]; 6],
    };
    for (level, origin) in frame.origins.iter().enumerate() {
        gpu.origins[level] = [origin[0], origin[1], 0, 0];
        gpu.planes[level] = planes_round(frame.level_box(level));
        gpu.moving_planes[level] = planes_round(frame.moving_box(level));
    }
    // The pages drawn, then those what moves stands in, each with its slot in the pool it is drawn into.
    let drawn = frame.drawn.iter().map(|(key, slot)| (key, *slot));
    let moving = frame.moving.iter().map(|(key, _, slot)| (key, *slot));
    for (place, (key, slot)) in gpu.pages.iter_mut().zip(drawn.chain(moving)) {
        *place = [i32::from(key.level), i32::from(slot), key.x, key.y];
    }
    gpu
}

/// A view's virtual shadow maps, kept from frame to frame.
pub(crate) struct VirtualShadows {
    basis: Option<Basis>,
    depth_min: f32,
    pages: HashMap<PageKey, Page>,
    free: Vec<u16>,
    /// The windows' origins each frame in flight asked for its pages with.
    asked_with: [Option<[[i32; 2]; LEVELS]>; FRAMES_IN_FLIGHT],
}

impl Default for VirtualShadows {
    fn default() -> Self {
        Self {
            basis: None,
            depth_min: 0.0,
            pages: HashMap::new(),
            free: (0..SLOTS as u16).rev().collect(),
            asked_with: [None; FRAMES_IN_FLIGHT],
        }
    }
}

/// What a frame brings the virtual shadows: the sun's direction, the
/// camera's place, the frame and its slot in flight, what that slot asked
/// for last time round (a bit a page of each window), where what moves
/// stands and where what stands still changed, and whether everything did.
pub(crate) struct VsmInput<'a> {
    pub sun: Vec3,
    pub camera: Vec3,
    pub frame_index: u64,
    pub slot: usize,
    pub asked: Option<&'a [u32]>,
    pub moving: &'a [[f32; 4]],
    pub changed: &'a [[f32; 4]],
    pub everything: bool,
}

impl VirtualShadows {
    /// Forgets every page, as when the pool is made anew.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    /// Brings the pages up to this frame: gives the pages asked for slots,
    /// marks those whose casters changed, and says which to draw.
    pub fn update(&mut self, input: VsmInput) -> VsmFrame {
        let turned = self.basis.is_none_or(|basis| {
            basis.direction.dot(input.sun.normalize_or(Vec3::NEG_Y)) < TURN_DEGREES.to_radians().cos()
        });
        let along = |basis: &Basis| basis.direction.dot(input.camera);
        let shallow = self.basis.as_ref().is_some_and(|basis| {
            let middle = self.depth_min + DEPTH_RANGE * 0.5;
            (along(basis) - middle).abs() > DEPTH_STEP * 1.5
        });
        if turned || shallow {
            tracing::trace!(turned, shallow, "virtual shadows: every page drawn again");
            let asked_with = self.asked_with;
            self.forget();
            // What the frames in flight asked for still says where they looked, though their pages are gone.
            self.asked_with = asked_with;
            self.basis = Some(Basis::of(input.sun));
        }
        let basis = self.basis.expect("made above");
        if turned || shallow || self.pages.is_empty() {
            self.depth_min = (along(&basis) / DEPTH_STEP).round() * DEPTH_STEP - DEPTH_RANGE * 0.5;
        }
        let [cx, cy] = basis.across(input.camera);
        let origins: [[i32; 2]; LEVELS] = std::array::from_fn(|level| {
            let metres = page_metres(level);
            [(cx / metres).floor() as i32 - WINDOW / 2, (cy / metres).floor() as i32 - WINDOW / 2]
        });
        let within = |key: &PageKey| {
            let [ox, oy] = origins[key.level as usize];
            (ox..ox + WINDOW).contains(&key.x) && (oy..oy + WINDOW).contains(&key.y)
        };

        // What the view asked for when this slot was last in flight, where it looked then, and the pages kept round
        // the camera.
        let mut asked: HashSet<PageKey> = HashSet::new();
        if let (Some(bits), Some(then)) = (input.asked, self.asked_with[input.slot]) {
            for (word, &value) in bits.iter().enumerate().filter(|(_, value)| **value != 0) {
                for bit in (0..32).filter(|bit| value >> bit & 1 != 0) {
                    let index = word * 32 + bit;
                    let level = index / WINDOW_PAGES;
                    let at = (index % WINDOW_PAGES) as i32;
                    let [ox, oy] = then[level];
                    asked.insert(PageKey { level: level as u8, x: ox + at % WINDOW, y: oy + at / WINDOW });
                }
            }
        }
        self.asked_with[input.slot] = Some(origins);
        for (level, round) in KEPT_ROUND {
            let metres = page_metres(level);
            let reach = (round / metres).ceil() as i32;
            let (px, py) = ((cx / metres).floor() as i32, (cy / metres).floor() as i32);
            for y in py - reach..=py + reach {
                for x in px - reach..=px + reach {
                    asked.insert(PageKey { level: level as u8, x, y });
                }
            }
        }
        asked.retain(within);

        // Pages out of their window go.
        let free = &mut self.free;
        self.pages.retain(|key, page| {
            let kept = within(key);
            if !kept {
                free.push(page.slot);
            }
            kept
        });

        // Whatever came, went, stopped or started moving, or changed, in a page draws its still part again.
        if input.everything {
            tracing::trace!("virtual shadows: everything changed");
            self.pages.values_mut().for_each(|page| page.stale = true);
        }
        if !input.changed.is_empty() {
            let widest = input.changed.iter().map(|sphere| sphere[3]).fold(0.0f32, f32::max);
            tracing::trace!(changed = input.changed.len(), widest, "virtual shadows: what stands still changed");
        }
        let changed: Vec<PageKey> = input.changed.iter().flat_map(|sphere| self.held_under(&basis, *sphere)).collect();
        for key in changed {
            if let Some(page) = self.pages.get_mut(&key) {
                page.stale = true;
            }
        }

        // Each page asked for gets a slot, the coarsest first, as they stand in for the finer until those are drawn;
        // slots go first to pages nobody asked for, the longest unused first.
        let mut wanted: Vec<PageKey> = asked.iter().copied().collect();
        wanted.sort_by_key(|key| (std::cmp::Reverse(key.level), key.x, key.y));
        for key in wanted {
            if let Some(page) = self.pages.get_mut(&key) {
                page.used = input.frame_index;
                continue;
            }
            let slot = match self.free.pop() {
                Some(slot) => slot,
                None => {
                    let unused = self.pages.iter().filter(|(_, page)| page.used < input.frame_index);
                    let Some((&old, _)) = unused.min_by_key(|(_, page)| page.used) else { break };
                    self.pages.remove(&old).expect("found above").slot
                }
            };
            self.pages.insert(key, Page { slot, drawn: false, stale: false, used: input.frame_index });
        }

        // Those never drawn first, the coarsest first, then those gone stale, the finest first, as many as a frame draws.
        let mut drawing: Vec<(PageKey, bool)> = self
            .pages
            .iter()
            .filter(|(_, page)| !page.drawn || page.stale)
            .map(|(key, page)| (*key, page.drawn))
            .collect();
        drawing.sort_by_key(|&(key, drawn)| {
            let order = if drawn { key.level as usize } else { LEVELS - key.level as usize };
            (drawn, order, key.x, key.y)
        });
        drawing.truncate(PAGES_A_FRAME);
        let mut drawn: Vec<(PageKey, u16)> = drawing
            .iter()
            .map(|(key, _)| {
                let page = self.pages.get_mut(key).expect("listed above");
                (page.drawn, page.stale) = (true, false);
                (*key, page.slot)
            })
            .collect();
        drawn.sort_by_key(|(key, _)| *key);

        // The pages what moves stands in, the finest first, as many as the moving pool holds: each a copy of its still
        // part with what moves drawn over it, this frame.
        let mut touched: Vec<PageKey> =
            input.moving.iter().flat_map(|sphere| self.held_under(&basis, *sphere)).collect();
        touched.sort();
        touched.dedup();
        let moving: Vec<(PageKey, u16, u16)> = touched
            .into_iter()
            .filter_map(|key| Some((key, self.pages.get(&key).filter(|page| page.drawn)?.slot)))
            .take(MOVING_SLOTS)
            .enumerate()
            .map(|(at, (key, slot))| (key, slot, at as u16))
            .collect();

        let mut table = vec![0u32; LEVELS * WINDOW_PAGES];
        let place = |key: &PageKey| {
            let [ox, oy] = origins[key.level as usize];
            key.level as usize * WINDOW_PAGES + ((key.y - oy) * WINDOW + key.x - ox) as usize
        };
        for (key, page) in self.pages.iter().filter(|(_, page)| page.drawn) {
            table[place(key)] = u32::from(page.slot) + 1;
        }
        for (key, _, moving_slot) in &moving {
            table[place(key)] |= (u32::from(*moving_slot) + 1) << 16;
        }
        VsmFrame {
            basis: Some(basis),
            depth_min: self.depth_min,
            origins,
            table,
            drawn,
            moving,
            held: self.pages.len(),
        }
    }

    /// The pages held under a sphere, in the levels it is drawn into: those
    /// it is at least two texels of, as the cull draws no smaller caster
    /// (`cullVirtualShadow`). One wider than a level's window covers all of it.
    fn held_under(&self, basis: &Basis, sphere: [f32; 4]) -> Vec<PageKey> {
        let [x, y] = basis.across(Vec3::new(sphere[0], sphere[1], sphere[2]));
        let mut keys = Vec::new();
        let drawn_in = |level: &usize| sphere[3] >= 2.0 * page_metres(*level) / PAGE as f32;
        for level in (0..LEVELS).filter(drawn_in) {
            let metres = page_metres(level);
            let span = |middle: f32| {
                ((middle - sphere[3]) / metres).floor() as i32..=((middle + sphere[3]) / metres).floor() as i32
            };
            let (columns, rows) = (span(x), span(y));
            if columns.clone().count() > WINDOW as usize {
                keys.extend(self.pages.keys().filter(|key| key.level as usize == level));
                continue;
            }
            for row in rows {
                for column in columns.clone() {
                    let key = PageKey { level: level as u8, x: column, y: row };
                    if self.pages.contains_key(&key) {
                        keys.push(key);
                    }
                }
            }
        }
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(camera: Vec3, frame_index: u64, asked: Option<&'a [u32]>, moving: &'a [[f32; 4]]) -> VsmInput<'a> {
        VsmInput {
            sun: Vec3::new(0.3, -1.0, 0.2),
            camera,
            frame_index,
            slot: (frame_index % FRAMES_IN_FLIGHT as u64) as usize,
            asked,
            moving,
            changed: &[],
            everything: false,
        }
    }

    /// The words asking for page `(x, y)` of the window of `level`.
    fn asking(level: usize, x: i32, y: i32) -> Vec<u32> {
        let mut words = vec![0u32; REQUEST_WORDS];
        let index = level * WINDOW_PAGES + (y * WINDOW + x) as usize;
        words[index / 32] |= 1 << (index % 32);
        words
    }

    #[test]
    fn the_kept_coarse_pages_are_drawn_first_and_kept_after() {
        let mut vsm = VirtualShadows::default();
        let first = vsm.update(input(Vec3::ZERO, 1, None, &[]));
        assert!(!first.drawn.is_empty() && first.drawn.len() <= PAGES_A_FRAME);
        assert!(first.drawn.iter().all(|(key, _)| key.level == 9 || key.level == 7), "only the kept coarse pages");
        let mut frame = 2;
        while vsm.pages.values().any(|page| !page.drawn) {
            vsm.update(input(Vec3::ZERO, frame, None, &[]));
            frame += 1;
        }
        let still = vsm.update(input(Vec3::ZERO, frame, None, &[]));
        assert!(still.drawn.is_empty(), "nothing drawn again while nothing changes");
        assert!(still.table.iter().filter(|&&slot| slot != 0).count() == still.held);
    }

    #[test]
    fn a_page_asked_for_is_drawn_once_kept_while_something_moves_in_it_and_drawn_again_when_it_stops() {
        let mut vsm = VirtualShadows::default();
        // Twice round the frames in flight, so each slot knows where it looked.
        for frame in 1..=20 {
            vsm.update(input(Vec3::ZERO, frame, None, &[]));
        }
        let words = asking(0, 32, 32);
        let frame = vsm.update(input(Vec3::ZERO, 21, Some(&words), &[]));
        assert_eq!(frame.drawn.len(), 1, "the page asked for: {:?}", frame.drawn);
        let key = frame.drawn[0].0;
        assert_eq!(key.level, 0);
        let again = vsm.update(input(Vec3::ZERO, 22, Some(&words), &[]));
        assert!(again.drawn.is_empty(), "kept");
        // A ball in the page.
        let basis = again.basis.unwrap();
        let metres = page_metres(0);
        let middle = basis.right * (key.x as f32 + 0.5) * metres + basis.up * (key.y as f32 + 0.5) * metres;
        let ball = [middle.x, middle.y, middle.z, 0.05];
        let moved = vsm.update(input(Vec3::ZERO, 23, Some(&words), &[ball]));
        assert!(moved.drawn.is_empty(), "its still part kept while the ball moves");
        let copied = moved.moving.iter().find(|(page, ..)| *page == key).expect("copied with the ball drawn over it");
        let at = (key.y - moved.origins[0][1]) * WINDOW + key.x - moved.origins[0][0];
        assert_eq!(moved.table[at as usize] >> 16, u32::from(copied.2) + 1, "the table says to read the copy");
        // The ball stops: what stands still changed there, so the page is drawn again, and copied no more.
        let rested = [ball];
        let mut stopped = input(Vec3::ZERO, 24, Some(&words), &[]);
        stopped.changed = &rested;
        let stopped = vsm.update(stopped);
        assert!(stopped.drawn.iter().any(|(drawn, _)| *drawn == key), "drawn again where it came to rest");
        assert!(stopped.moving.is_empty());
        let after = vsm.update(input(Vec3::ZERO, 25, Some(&words), &[]));
        assert!(after.drawn.is_empty());
    }

    #[test]
    fn the_sun_turning_draws_every_page_again_and_a_little_turn_none() {
        let mut vsm = VirtualShadows::default();
        for frame in 1..=20 {
            vsm.update(input(Vec3::ZERO, frame, None, &[]));
        }
        let mut nudged = input(Vec3::ZERO, 21, None, &[]);
        nudged.sun = Vec3::new(0.3001, -1.0, 0.2);
        assert!(vsm.update(nudged).drawn.is_empty(), "a hair's turn keeps every page");
        let mut turned = input(Vec3::ZERO, 22, None, &[]);
        turned.sun = Vec3::new(0.5, -1.0, 0.2);
        assert_eq!(
            vsm.update(turned).drawn.len(),
            PAGES_A_FRAME,
            "turned, all of them again, a frame's worth at a time"
        );
    }

    #[test]
    fn pages_keep_their_place_as_the_camera_moves_and_those_left_behind_go() {
        let mut vsm = VirtualShadows::default();
        for frame in 1..=20 {
            vsm.update(input(Vec3::ZERO, frame, None, &[]));
        }
        let held = vsm.pages.len();
        // A few metres on, only the pages coming into the kept rounds are new.
        let near = vsm.update(input(Vec3::new(3.0, 0.0, 0.0), 21, None, &[]));
        assert!(near.drawn.len() < held / 4, "{} of {held} drawn again", near.drawn.len());
        // Far off, the old pages go and their slots are free again.
        for frame in 22..=60 {
            vsm.update(input(Vec3::new(9000.0, 0.0, 0.0), frame, None, &[]));
        }
        assert!(vsm.pages.len() + vsm.free.len() == SLOTS, "every slot held or free");
    }
}
