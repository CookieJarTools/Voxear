//! Virtual shadow maps for local lights.
//!
//! Each view of a lamp's shadow, a spot light's one or a face of a point or
//! shaped light's cube, is a virtual shadow map `FINEST` texels a side,
//! eight times the atlas's tile, with `MIPS` levels each half as fine as the
//! last. A level is cut into pages of `vsm::PAGE` texels, and only the pages
//! the view needs are drawn, into slots of a pool texture, kept from frame
//! to frame while the lamp stands still and nothing drawn in them changes.
//!
//! Each frame the view's depth asks for pages (`markLampsMain` in
//! `shaders/shadow.slang`): each pixel within a lamp's reach the page of the
//! level whose texels are no wider than the pixel is there, but no coarser
//! than the atlas's views, and the two coarser, which soft shadows read. The CPU reads what a frame asked for
//! once its frame in flight comes round again, gives each page a slot, the
//! coarsest first, and draws those newly given and those gone stale, up to
//! `PAGES_A_FRAME` a frame. The coarsest level of every view is always
//! kept, for what the view cannot ask for: the fog's light shafts, the
//! bounced light's rays, and points no page is drawn for yet.
//!
//! The pages hold what stands still, as the kept atlas does
//! (`shadow_cache`): what moves is drawn each frame into the lamp's views of
//! the atlas, and shading takes the darker of the two. A lamp that moves, or
//! whose coarsest pages are not drawn yet, is drawn whole into the atlas
//! instead, as lamps are without virtual shadows.
//!
//! A page's casters are drawn into its slot with `vsm::BORDER` texels of
//! border all round, so every tap of a filter centred in the page reads its
//! own slot.

use std::collections::{HashMap, HashSet};

use voxear_core::math::{Mat4, Vec3, Vec4};
use voxear_rhi::FRAMES_IN_FLIGHT;

use crate::gpu::{LOCAL_SHADOW_LIGHTS, LOCAL_SHADOW_SLOT_VIEWS, Mat4Cols};
use crate::vsm::{BORDER, PAGE, SLOT};

/// Texels a side of a view's finest level, and its levels, finest first.
pub const FINEST: u32 = 4096;
pub const MIPS: usize = 6;
/// Pages a side of a view's finest level.
const ACROSS: u32 = FINEST / PAGE;
/// Pages of every level of a view, and of every view of a slot.
pub const VIEW_PAGES: usize = 1365;
const SLOT_PAGES: usize = VIEW_PAGES * LOCAL_SHADOW_SLOT_VIEWS as usize;
/// Words of the page table: a word a page of every view of every slot.
pub const TABLE_WORDS: usize = SLOT_PAGES * LOCAL_SHADOW_LIGHTS;
/// Words of a frame's requests: a bit a page.
pub const REQUEST_WORDS: usize = TABLE_WORDS.div_ceil(32);
/// Slots a side of the pool, and its texels a side.
pub const POOL_SLOTS: u32 = 32;
pub const POOL: u32 = SLOT * POOL_SLOTS;
const SLOTS: usize = (POOL_SLOTS * POOL_SLOTS) as usize;
/// The most pages drawn in a frame.
pub const PAGES_A_FRAME: usize = 64;
/// The level whose pages every view keeps.
const KEPT_MIP: usize = MIPS - 1;

/// Pages a side of level `mip` of a view.
fn across(mip: usize) -> u32 {
    ACROSS >> mip
}

/// Where level `mip`'s pages start among a view's: after every finer level's.
fn mip_base(mip: usize) -> usize {
    VIEW_PAGES - (VIEW_PAGES >> (2 * mip))
}

/// A page of a lamp's view: the lamp's slot, the view among the slot's, its
/// level, and its column and row there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LampPage {
    pub slot: u8,
    pub face: u8,
    pub mip: u8,
    pub x: u16,
    pub y: u16,
}

impl LampPage {
    /// Its word in the page table, and its bit in the requests.
    pub fn index(&self) -> usize {
        let mip = self.mip as usize;
        self.slot as usize * SLOT_PAGES
            + self.face as usize * VIEW_PAGES
            + mip_base(mip)
            + self.y as usize * across(mip) as usize
            + self.x as usize
    }

    /// The page at word `index` of the page table.
    fn at(index: usize) -> Self {
        let slot = index / SLOT_PAGES;
        let face = index % SLOT_PAGES / VIEW_PAGES;
        let within = index % VIEW_PAGES;
        let mip = (0..MIPS).rev().find(|&mip| within >= mip_base(mip)).expect("level 0 starts at 0");
        let at = (within - mip_base(mip)) as u32;
        let side = across(mip);
        Self { slot: slot as u8, face: face as u8, mip: mip as u8, x: (at % side) as u16, y: (at / side) as u16 }
    }
}

/// A page held in a slot of the pool: whether it has been drawn there,
/// whether something drawn in it changed since, and the last frame it was
/// asked for.
#[derive(Clone, Copy, Debug)]
struct Page {
    slot: u16,
    drawn: bool,
    stale: bool,
    used: u64,
}

/// What a frame's lamp pages draw and read.
#[derive(Clone, Debug, Default)]
pub struct LampFrame {
    /// Each page of each view of each slot: its slot of the pool plus one,
    /// or 0 where none is drawn.
    pub table: Vec<u32>,
    /// The pages drawn this frame, by lamp, and each one's slot.
    pub drawn: Vec<(LampPage, u16)>,
    /// The lamps' slots, a bit each, whose shadows of what stands still
    /// are read from their pages: those standing still whose coarsest pages
    /// are drawn.
    pub ready: u32,
    /// How many pages are held in the pool.
    pub held: usize,
}

impl LampFrame {
    /// The lamps' slots, a bit each, with pages drawn this frame.
    pub fn drawing(&self) -> u32 {
        self.drawn.iter().fold(0, |bits, (page, _)| bits | 1 << page.slot)
    }
}

/// What the shaders read of a frame's lamp pages: the pages drawn, each
/// slot's views, and the buffers and pool.
pub(crate) fn gpu(
    frame: &LampFrame,
    views: &[(u32, u32)],
    tan_halves: &[f32; LOCAL_SHADOW_LIGHTS],
    (table, requests): (u64, u64),
    pool: u32,
) -> crate::gpu::GpuLampShadows {
    let mut gpu = crate::gpu::GpuLampShadows {
        table,
        requests,
        pool,
        drawn: frame.drawn.len() as u32,
        ..crate::gpu::GpuLampShadows::default()
    };
    for (slot, &(first, count)) in views.iter().enumerate().take(LOCAL_SHADOW_LIGHTS) {
        gpu.lights[slot] = [first as f32, count as f32, tan_halves[slot], 0.0];
    }
    for (place, (page, _)) in gpu.pages.iter_mut().zip(&frame.drawn) {
        let view = u32::from(page.slot) * LOCAL_SHADOW_SLOT_VIEWS + u32::from(page.face);
        *place = [view as i32, i32::from(page.mip), i32::from(page.x), i32::from(page.y)];
    }
    gpu
}

/// What a frame brings the lamp pages: each slot's views, a projection a
/// view, empty for a slot without a lamp; the frame and its slot in flight;
/// what that slot asked for last time round; and where what stands still
/// changed since last frame, and whether everything did.
pub(crate) struct LampInput<'a> {
    pub views: Vec<Vec<Mat4Cols>>,
    pub frame_index: u64,
    pub slot: usize,
    pub asked: Option<&'a [u32]>,
    pub changed: &'a [[f32; 4]],
    pub everything: bool,
}

/// A view's lamp pages, kept from frame to frame.
pub(crate) struct LampPages {
    pages: HashMap<LampPage, Page>,
    free: Vec<u16>,
    /// Each slot's views its pages were drawn for, and how many times
    /// they changed, which the requests carry.
    views: [Vec<Mat4Cols>; LOCAL_SHADOW_LIGHTS],
    turns: [u32; LOCAL_SHADOW_LIGHTS],
    /// The slots' turns each frame in flight asked for its pages with.
    asked_with: [Option<[u32; LOCAL_SHADOW_LIGHTS]>; FRAMES_IN_FLIGHT],
}

impl Default for LampPages {
    fn default() -> Self {
        Self {
            pages: HashMap::new(),
            free: (0..SLOTS as u16).rev().collect(),
            views: Default::default(),
            turns: [0; LOCAL_SHADOW_LIGHTS],
            asked_with: [None; FRAMES_IN_FLIGHT],
        }
    }
}

impl LampPages {
    /// Forgets every page, as when the pool is made anew.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    /// Forgets a slot's pages.
    fn forget_slot(&mut self, slot: usize) {
        let free = &mut self.free;
        self.pages.retain(|page, held| {
            let kept = page.slot as usize != slot;
            if !kept {
                free.push(held.slot);
            }
            kept
        });
    }

    /// Brings the pages up to this frame: forgets those of lamps that came,
    /// went or moved, marks those whose casters changed, gives the pages
    /// asked for slots, and says which to draw.
    pub fn update(&mut self, input: LampInput) -> LampFrame {
        // A lamp that came, went or moved since last frame has its pages drawn again once it stands still.
        let mut still = [false; LOCAL_SHADOW_LIGHTS];
        for slot in 0..LOCAL_SHADOW_LIGHTS {
            let now = input.views.get(slot).cloned().unwrap_or_default();
            if self.views[slot] != now {
                self.forget_slot(slot);
                self.views[slot] = now;
                self.turns[slot] = self.turns[slot].wrapping_add(1);
            } else {
                still[slot] = !self.views[slot].is_empty();
            }
        }

        // What the view asked for when this slot was last in flight, of lamps that have not moved since; and every
        // still lamp's coarsest pages.
        let mut asked: HashSet<LampPage> = HashSet::new();
        if let (Some(bits), Some(then)) = (input.asked, self.asked_with[input.slot]) {
            for (word, &value) in bits.iter().enumerate().filter(|(_, value)| **value != 0) {
                for bit in (0..32).filter(|bit| value >> bit & 1 != 0) {
                    let index = word * 32 + bit;
                    if index < TABLE_WORDS {
                        let page = LampPage::at(index);
                        let slot = page.slot as usize;
                        if then[slot] == self.turns[slot] && (page.face as usize) < self.views[slot].len() {
                            asked.insert(page);
                        }
                    }
                }
            }
        }
        self.asked_with[input.slot] = Some(self.turns);
        for slot in (0..LOCAL_SHADOW_LIGHTS).filter(|&slot| still[slot]) {
            let side = across(KEPT_MIP) as u16;
            for face in 0..self.views[slot].len() as u8 {
                for y in 0..side {
                    for x in 0..side {
                        asked.insert(LampPage { slot: slot as u8, face, mip: KEPT_MIP as u8, x, y });
                    }
                }
            }
        }
        asked.retain(|page| still[page.slot as usize]);

        // Whatever came, went or changed of what stands still in a page draws it again.
        if input.everything {
            self.pages.values_mut().for_each(|page| page.stale = true);
        }
        for sphere in input.changed {
            for page in self.held_under(*sphere) {
                if let Some(held) = self.pages.get_mut(&page) {
                    held.stale = true;
                }
            }
        }

        // Each page asked for gets a slot, the coarsest first; slots go first to pages nobody asked for, the longest
        // unused first.
        let mut wanted: Vec<LampPage> = asked.into_iter().collect();
        wanted.sort_by_key(|page| (std::cmp::Reverse(page.mip), *page));
        for page in wanted {
            if let Some(held) = self.pages.get_mut(&page) {
                held.used = input.frame_index;
                continue;
            }
            let slot = match self.free.pop() {
                Some(slot) => slot,
                None => {
                    let unused = self.pages.iter().filter(|(_, held)| held.used < input.frame_index);
                    let Some((&old, _)) = unused.min_by_key(|(_, held)| held.used) else { break };
                    self.pages.remove(&old).expect("found above").slot
                }
            };
            self.pages.insert(page, Page { slot, drawn: false, stale: false, used: input.frame_index });
        }

        // Those never drawn first, the coarsest first, then those gone stale, as many as a frame draws.
        let mut drawing: Vec<(LampPage, bool)> = self
            .pages
            .iter()
            .filter(|(_, held)| !held.drawn || held.stale)
            .map(|(page, held)| (*page, held.drawn))
            .collect();
        drawing.sort_by_key(|&(page, drawn)| (drawn, std::cmp::Reverse(page.mip), page));
        drawing.truncate(PAGES_A_FRAME);
        let mut drawn: Vec<(LampPage, u16)> = drawing
            .iter()
            .map(|(page, _)| {
                let held = self.pages.get_mut(page).expect("listed above");
                (held.drawn, held.stale) = (true, false);
                (*page, held.slot)
            })
            .collect();
        drawn.sort_by_key(|(page, _)| *page);

        let mut table = vec![0u32; TABLE_WORDS];
        for (page, held) in self.pages.iter().filter(|(_, held)| held.drawn) {
            table[page.index()] = u32::from(held.slot) + 1;
        }
        // A lamp is read from its pages once every view's coarsest level is drawn.
        let side = across(KEPT_MIP) as u16;
        let ready = (0..LOCAL_SHADOW_LIGHTS).filter(|&slot| still[slot]).fold(0u32, |bits, slot| {
            let coarsest = (0..self.views[slot].len() as u8).all(|face| {
                (0..side).all(|y| {
                    (0..side).all(|x| {
                        let page = LampPage { slot: slot as u8, face, mip: KEPT_MIP as u8, x, y };
                        self.pages.get(&page).is_some_and(|held| held.drawn)
                    })
                })
            });
            if coarsest { bits | 1 << slot } else { bits }
        });
        LampFrame { table, drawn, ready, held: self.pages.len() }
    }

    /// The pages held under a sphere: in each view of each lamp it lies
    /// within reach of, those its box covers on each level. A sphere round
    /// the lamp, or crossing a view's near plane, covers the whole view.
    fn held_under(&self, sphere: [f32; 4]) -> Vec<LampPage> {
        let center = Vec3::new(sphere[0], sphere[1], sphere[2]);
        let radius = sphere[3];
        let mut pages = Vec::new();
        for (slot, views) in self.views.iter().enumerate() {
            for (face, view) in views.iter().enumerate() {
                let Some((low, high)) = uv_box(Mat4::from_cols_array_2d(view), center, radius) else { continue };
                for mip in 0..MIPS {
                    let side = across(mip);
                    let span = |low: f32, high: f32| {
                        let first = ((low * side as f32).floor().max(0.0) as u32).min(side - 1);
                        let last = ((high * side as f32).floor().max(0.0) as u32).min(side - 1);
                        first..=last
                    };
                    let (columns, rows) = (span(low[0], high[0]), span(low[1], high[1]));
                    for y in rows {
                        for x in columns.clone() {
                            let page = LampPage {
                                slot: slot as u8,
                                face: face as u8,
                                mip: mip as u8,
                                x: x as u16,
                                y: y as u16,
                            };
                            if self.pages.contains_key(&page) {
                                pages.push(page);
                            }
                        }
                    }
                }
            }
        }
        pages
    }
}

/// The box a sphere covers in a view, as its texture lays it, from 0 to 1
/// either way, with a page's border round it; `None` where it lies wholly
/// outside the view, and the whole view where it crosses its near plane.
fn uv_box(view_proj: Mat4, center: Vec3, radius: f32) -> Option<([f32; 2], [f32; 2])> {
    // Outside any side of the view, or before its near plane or past its far one.
    let [r0, r1, r2, r3] = [0, 1, 2, 3].map(|row| view_proj.row(row));
    for plane in [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2] {
        let length = plane.truncate().length().max(1e-6);
        if (plane.truncate().dot(center) + plane.w) / length < -radius {
            return None;
        }
    }
    let mut low = [f32::MAX; 2];
    let mut high = [f32::MIN; 2];
    let mut nearest = f32::MAX;
    for corner in 0..8 {
        let sign = |bit: u32| if corner >> bit & 1 != 0 { 1.0 } else { -1.0 };
        let point = center + Vec3::new(sign(0), sign(1), sign(2)) * radius;
        let clip = view_proj * Vec4::new(point.x, point.y, point.z, 1.0);
        if clip.w <= 1e-4 {
            return Some(([0.0; 2], [1.0; 2]));
        }
        let uv = [clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5];
        nearest = nearest.min(clip.z / clip.w);
        for axis in 0..2 {
            low[axis] = low[axis].min(uv[axis]);
            high[axis] = high[axis].max(uv[axis]);
        }
    }
    let border = BORDER as f32 / FINEST as f32 * (1 << (MIPS - 1)) as f32;
    let low = [low[0] - border, low[1] - border];
    let high = [high[0] + border, high[1] + border];
    let outside = high[0] < 0.0 || high[1] < 0.0 || low[0] > 1.0 || low[1] > 1.0 || nearest > 1.0;
    (!outside).then_some((low, high))
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;
    use crate::shadows::{LocalLight, local_views};

    fn lamp_views(x: f32) -> Vec<Mat4Cols> {
        let light = LocalLight {
            position: Vec3::new(x, 2.0, 0.0),
            rotation: Quat::IDENTITY,
            range: 8.0,
            near: 0.1,
            cone_degrees: None,
        };
        local_views(&light).into_iter().map(crate::gpu::cols).collect()
    }

    fn input<'a>(views: Vec<Vec<Mat4Cols>>, frame_index: u64, asked: Option<&'a [u32]>) -> LampInput<'a> {
        LampInput {
            views,
            frame_index,
            slot: (frame_index % FRAMES_IN_FLIGHT as u64) as usize,
            asked,
            changed: &[],
            everything: false,
        }
    }

    fn asking(page: LampPage) -> Vec<u32> {
        let mut words = vec![0u32; REQUEST_WORDS];
        words[page.index() / 32] |= 1 << (page.index() % 32);
        words
    }

    #[test]
    fn a_pages_place_in_the_table_comes_back_as_the_page() {
        for page in [
            LampPage { slot: 0, face: 0, mip: 0, x: 0, y: 0 },
            LampPage { slot: 3, face: 5, mip: 0, x: 31, y: 17 },
            LampPage { slot: 7, face: 2, mip: 3, x: 2, y: 3 },
            LampPage { slot: 7, face: 5, mip: 5, x: 0, y: 0 },
        ] {
            assert_eq!(LampPage::at(page.index()), page);
        }
        assert_eq!(LampPage { slot: 7, face: 5, mip: 5, x: 0, y: 0 }.index(), TABLE_WORDS - 1);
    }

    #[test]
    fn a_still_lamp_draws_its_coarsest_pages_then_those_asked_for_and_keeps_them() {
        let mut lamps = LampPages::default();
        let views = vec![lamp_views(0.0)];
        let first = lamps.update(input(views.clone(), 1, None));
        assert!(first.drawn.is_empty() && first.ready == 0, "a lamp new this frame waits a frame");
        let second = lamps.update(input(views.clone(), 2, None));
        assert_eq!(second.drawn.len(), 6, "each face's coarsest page");
        assert_eq!(second.ready, 1);
        let fine = LampPage { slot: 0, face: 2, mip: 0, x: 10, y: 12 };
        let words = asking(fine);
        // Asked for in frame 2's slot, read when it comes round at frame 4.
        lamps.update(input(views.clone(), 3, None));
        let asked = lamps.update(input(views.clone(), 4, Some(&words)));
        assert_eq!(asked.drawn.iter().map(|(page, _)| *page).collect::<Vec<_>>(), [fine]);
        assert_ne!(asked.table[fine.index()], 0);
        let after = lamps.update(input(views.clone(), 5, None));
        assert!(after.drawn.is_empty() && after.table[fine.index()] != 0, "kept, not drawn again");
    }

    #[test]
    fn a_change_under_a_page_draws_it_again_and_a_moving_lamp_forgets_its_pages() {
        let mut lamps = LampPages::default();
        let views = vec![lamp_views(0.0)];
        for frame in 1..4 {
            lamps.update(input(views.clone(), frame, None));
        }
        // Something below the lamp: in the face looking down.
        let changed = [[0.0, 0.0, 0.0, 0.3]];
        let after = lamps.update(LampInput { changed: &changed, ..input(views.clone(), 4, None) });
        let faces: Vec<u8> = after.drawn.iter().map(|(page, _)| page.face).collect();
        assert_eq!(faces, [3], "only the face looking down");
        let moved = lamps.update(input(vec![lamp_views(1.0)], 5, None));
        assert!(moved.held == 0 && moved.ready == 0, "a lamp that moved keeps nothing");
    }
}
