//! What comes loose of what was built in an endless Voxel Volume when its
//! supports are dug out ([`loosen`]), and blasts that dig out what was
//! built ([`blast`]).
//!
//! Every voxel an edit fills, a print prints or a game places is built;
//! the ground as it is made holds it up, and with the volume's
//! [`VoxelAnchor`] earth filled or placed holds as the ground does. What is
//! built is held where it touches what holds by a face, or touches by
//! faces other built voxels that are held. Once some are dug out, only what
//! touched them can have come loose: the check starts there and goes over
//! half-metre bricks first, the bricks whose built voxels touch across
//! their faces, then over voxels, the way the bricks say is nearest to
//! what holds. Whatever finds nothing that holds comes loose: a balcony, a
//! wall over a breach, a bridge's span. It is taken out of the volume a
//! voxel at a time and given back as [`VoxelPiece`]s to fall as felled
//! trees do, or as crumbs too few to be pieces.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap, HashSet, VecDeque},
    sync::Arc,
};

use voxear_core::math::{IVec3, Vec3};

use super::{
    GROUND_COLORS, MOST_VOXELS_A_METRE, PAVING_COLORS, VoxelChanges, VoxelHeld, VoxelMaterial, VoxelPiece, VoxelVolume,
    endless::{GENERATED_COLORS, places_of, tinted_colours},
    solid::{Ground, Made},
    strain::{VoxelStrain, VoxelStrainDone, VoxelStrainMemory, strain},
    trees,
};

/// What holds up what is built in an endless Voxel Volume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelAnchor {
    /// The ground as it is made, and earth filled or placed untinted (dirt,
    /// grass, rock, sand, snow, gravel): a bank raised holds as the hill
    /// does. Pavings, prints and what is tinted are built.
    #[default]
    Earth,
    /// The ground as it is made alone: a bank raised comes loose too.
    Ground,
    /// Everything solid: nothing comes loose.
    Everything,
}

impl VoxelAnchor {
    pub const ALL: [Self; 3] = [Self::Earth, Self::Ground, Self::Everything];

    pub fn name(self) -> &'static str {
        match self {
            Self::Earth => "Earth",
            Self::Ground => "Ground",
            Self::Everything => "Everything",
        }
    }
}

/// What a built voxel is made of, for how heavy and how strong it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelStuff {
    /// Earth heaped up: heavy and weak.
    Earth,
    /// Dressed stone and cobbles.
    #[default]
    Stone,
    /// Bricks laid in mortar.
    Brick,
    /// Timber: light, and strong pulled as pushed.
    Wood,
    /// Glass: brittle.
    Glass,
}

impl VoxelStuff {
    pub const ALL: [Self; 5] = [Self::Earth, Self::Stone, Self::Brick, Self::Wood, Self::Glass];

    pub fn name(self) -> &'static str {
        match self {
            Self::Earth => "Earth",
            Self::Stone => "Stone",
            Self::Brick => "Brick",
            Self::Wood => "Wood",
            Self::Glass => "Glass",
        }
    }

    /// How heavy it is, in kilograms a cubic metre.
    pub fn density(self) -> f32 {
        match self {
            Self::Earth => 1600.0,
            Self::Stone => 2400.0,
            Self::Brick => 1900.0,
            Self::Wood => 600.0,
            Self::Glass => 2500.0,
        }
    }

    /// What a material filled or placed is made of.
    pub fn of(material: VoxelMaterial) -> Self {
        match material {
            VoxelMaterial::Cobbles | VoxelMaterial::Flagstones | VoxelMaterial::Ashlar => Self::Stone,
            VoxelMaterial::Bricks => Self::Brick,
            VoxelMaterial::Planks => Self::Wood,
            _ => Self::Earth,
        }
    }
}

/// Whether `material` is earth, which holds with [`VoxelAnchor::Earth`].
pub fn is_earth(material: VoxelMaterial) -> bool {
    !material.is_paving()
}

/// What a voxel is, for what holds: open, holding, or built.
pub(super) const AIR: u8 = 0;
pub(super) const HOLDS: u8 = 1;
pub(super) const BUILT: u8 = 2;

/// A built voxel's colour and what it is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Look {
    pub colour: [u8; 4],
    pub stuff: VoxelStuff,
}

const B: i32 = 8;
const CELLS: usize = 512;

/// A brick's voxels: what each is, and each built one's look.
pub(super) struct Brick {
    pub class: [u8; CELLS],
    pub look: [u16; CELLS],
}

fn local_index(l: IVec3) -> usize {
    (l.x + B * (l.y + B * l.z)) as usize
}

/// The six ways a voxel's faces look.
pub(super) const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

/// What a brick's built voxels touch: whether any touches what holds, how
/// many there are, and for each side whether any touches a built voxel of
/// the brick that way, and by how many faces.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BrickTouch {
    pub built: u32,
    pub rooted: u32,
    pub sides: [u32; 6],
}

/// An endless volume's voxels as what holds and what is built, worked out
/// a brick at a time as they are asked for and kept.
pub(super) struct Grid<'a> {
    ground: Ground<'a>,
    volume: &'a VoxelVolume,
    anchor: VoxelAnchor,
    /// What prints are made of.
    printed: VoxelStuff,
    bricks: HashMap<IVec3, Box<Brick>>,
    touches: HashMap<IVec3, BrickTouch>,
    pub looks: Vec<Look>,
    look_at: HashMap<Look, u16>,
    made: Vec<Made>,
    pub voxel: f32,
    base: Vec<[u8; 4]>,
    /// The voxels taken out, which are open from then on.
    taken: HashSet<IVec3>,
}

impl<'a> Grid<'a> {
    pub fn new(volume: &'a VoxelVolume, woods: Option<&'a trees::Woods>) -> Self {
        let ground = Ground::new(volume, woods);
        let voxel = ground.voxel;
        let base = GROUND_COLORS.iter().chain(&GENERATED_COLORS).chain(&PAVING_COLORS).copied().collect();
        Self {
            ground,
            volume,
            anchor: volume.anchor,
            printed: volume.built,
            bricks: HashMap::new(),
            touches: HashMap::new(),
            looks: Vec::new(),
            look_at: HashMap::new(),
            made: Vec::new(),
            voxel,
            base,
            taken: HashSet::new(),
        }
    }

    fn look(&mut self, look: Look) -> u16 {
        if let Some(&at) = self.look_at.get(&look) {
            return at;
        }
        self.looks.push(look);
        let at = (self.looks.len() - 1) as u16;
        self.look_at.insert(look, at);
        at
    }

    /// The colour a material filled or placed has at `c`, tinted by `tint`.
    fn fill_colour(&self, material: VoxelMaterial, tint: Option<[u8; 3]>, c: IVec3) -> [u8; 4] {
        let own: Vec<[u8; 4]> = places_of(material).into_iter().map(|at| self.base[at]).collect();
        let shades = match tint {
            Some(tint) => tinted_colours(&own, tint),
            None => own,
        };
        // A paving's last is its joints'; any of the rest at each voxel.
        let n = if material.is_paving() { shades.len() - 1 } else { shades.len() }.max(1);
        let h = (c.x as u32).wrapping_mul(0x8DA6_B343)
            ^ (c.y as u32).wrapping_mul(0xD816_3841)
            ^ (c.z as u32).wrapping_mul(0xCB1A_B31F);
        shades[((h ^ (h >> 13)) % n as u32) as usize]
    }

    fn classify(&mut self, made: Made, c: IVec3) -> (u8, u16) {
        let volume = self.volume;
        match made {
            Made::Air | Made::Carved => (AIR, 0),
            Made::Ground | Made::Log => (HOLDS, 0),
            Made::Edit(e, VoxelHeld::Fill(material)) => {
                let edit = &volume.edits[e as usize];
                if self.anchor == VoxelAnchor::Earth && is_earth(material) && edit.tint.is_none() {
                    return (HOLDS, 0);
                }
                let colour = self.fill_colour(material, edit.tint, c);
                (BUILT, self.look(Look { colour, stuff: VoxelStuff::of(material) }))
            }
            Made::Edit(e, VoxelHeld::Colour(place)) => {
                let edit = &volume.edits[e as usize];
                let Some(printed) = edit.print() else { return (BUILT, 0) };
                let mut colour = printed.print.volume_colour(usize::from(place) - 1);
                if let Some(tint) = edit.tint {
                    let all: Vec<[u8; 4]> =
                        (0..printed.print.palette.len()).map(|i| printed.print.volume_colour(i)).collect();
                    colour = tinted_colours(&all, tint)[usize::from(place) - 1];
                }
                // See-through colours are glass.
                let stuff = if colour[3] & 15 != 0 { VoxelStuff::Glass } else { self.printed };
                (BUILT, self.look(Look { colour, stuff }))
            }
            Made::Edit(..) => (AIR, 0),
            Made::Placed(code) => {
                let Some(kind) = volume.changes.kinds().get(usize::from(code).wrapping_sub(1)).copied() else {
                    return (HOLDS, 0);
                };
                if self.anchor == VoxelAnchor::Earth && is_earth(kind.material) && kind.tint.is_none() {
                    return (HOLDS, 0);
                }
                let colour = match kind.tint {
                    // A piece stamped back is placed in its own colours.
                    Some([r, g, b]) if kind.material == VoxelMaterial::Dirt => [r, g, b, 255],
                    tint => self.fill_colour(kind.material, tint, c),
                };
                let stuff = if kind.material == VoxelMaterial::Dirt && kind.tint.is_some() {
                    self.printed
                } else {
                    VoxelStuff::of(kind.material)
                };
                (BUILT, self.look(Look { colour, stuff }))
            }
        }
    }

    /// The brick at `b`, worked out where it was not yet.
    pub fn brick(&mut self, b: IVec3) -> &Brick {
        if !self.bricks.contains_key(&b) {
            let mut made = std::mem::take(&mut self.made);
            self.ground.made_in(b * B, IVec3::splat(B), &mut made);
            let mut brick = Box::new(Brick { class: [AIR; CELLS], look: [0; CELLS] });
            for (i, m) in made.iter().enumerate() {
                let l = IVec3::new(i as i32 % B, (i as i32 / B) % B, i as i32 / (B * B));
                let (class, look) = self.classify(*m, b * B + l);
                brick.class[i] = class;
                brick.look[i] = look;
            }
            self.made = made;
            self.bricks.insert(b, brick);
        }
        &self.bricks[&b]
    }

    /// What voxel `c` is.
    pub fn class(&mut self, c: IVec3) -> u8 {
        let b = c.div_euclid(IVec3::splat(B));
        let l = c - b * B;
        self.brick(b).class[local_index(l)]
    }

    /// The look of built voxel `c`.
    pub fn look_of(&mut self, c: IVec3) -> u16 {
        let b = c.div_euclid(IVec3::splat(B));
        let l = c - b * B;
        self.brick(b).look[local_index(l)]
    }

    /// Takes voxel `c` out: open from now on.
    pub fn take(&mut self, c: IVec3) {
        let b = c.div_euclid(IVec3::splat(B));
        let l = c - b * B;
        self.brick(b);
        if let Some(brick) = self.bricks.get_mut(&b) {
            brick.class[local_index(l)] = AIR;
        }
        self.taken.insert(c);
        // What the brick and its neighbours touch is worked out again.
        self.touches.remove(&b);
        for side in SIDES {
            self.touches.remove(&(b + side));
        }
    }

    /// What the built voxels of brick `b` touch.
    pub fn touch(&mut self, b: IVec3) -> BrickTouch {
        if let Some(touch) = self.touches.get(&b) {
            return *touch;
        }
        let mut touch = BrickTouch::default();
        let own: Vec<u8> = self.brick(b).class.to_vec();
        if own.contains(&BUILT) {
            for side in SIDES {
                self.brick(b + side);
            }
            for i in 0..CELLS {
                if own[i] != BUILT {
                    continue;
                }
                touch.built += 1;
                let l = IVec3::new(i as i32 % B, (i as i32 / B) % B, i as i32 / (B * B));
                for (s, side) in SIDES.iter().enumerate() {
                    let n = l + *side;
                    let inside = n.cmpge(IVec3::ZERO).all() && n.cmplt(IVec3::splat(B)).all();
                    let class = if inside { own[local_index(n)] } else { self.class(b * B + n) };
                    match class {
                        HOLDS => touch.rooted += 1,
                        BUILT if !inside => touch.sides[s] += 1,
                        _ => {}
                    }
                }
            }
        }
        self.touches.insert(b, touch);
        touch
    }
}

/// The most bricks and voxels one check goes over: anything joined to more
/// is taken as held.
const MOST_BRICKS: usize = 60_000;
const MOST_VOXELS: usize = 6_000_000;
/// The fewest voxels that come loose as a piece: fewer are crumbs.
pub const CRUMBS: usize = 40;

/// What a check for what came loose did: what came loose as pieces, where
/// crumbs fell (a place in metres from the volume's corner for each brick
/// of them), how many bricks and voxels it went over, and how many voxels
/// came loose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelLoosened {
    pub pieces: Vec<VoxelPiece>,
    pub crumbs: Vec<Vec3>,
    pub bricks: usize,
    pub voxels: usize,
    pub loose: usize,
}

/// The bricks of built voxels joined to brick `start` across faces, as far
/// as `most`: each, and how many bricks it is from one whose voxels touch
/// what holds (none where none of them does).
pub(super) fn brick_component(grid: &mut Grid, start: IVec3, most: usize) -> (Vec<IVec3>, HashMap<IVec3, u32>, bool) {
    let mut seen: HashSet<IVec3> = HashSet::from([start]);
    let mut order = vec![start];
    let mut at = 0;
    let mut rooted = Vec::new();
    let mut whole = true;
    while at < order.len() {
        let b = order[at];
        at += 1;
        let touch = grid.touch(b);
        if touch.rooted > 0 {
            rooted.push(b);
        }
        for (s, side) in SIDES.iter().enumerate() {
            if touch.sides[s] > 0 && seen.insert(b + *side) {
                if order.len() >= most {
                    whole = false;
                    continue;
                }
                order.push(b + *side);
            }
        }
    }
    // How far each is from a rooted one, across the bricks.
    let mut far: HashMap<IVec3, u32> = rooted.iter().map(|b| (*b, 0)).collect();
    let mut queue: VecDeque<IVec3> = rooted.into_iter().collect();
    while let Some(b) = queue.pop_front() {
        let d = far[&b];
        let touch = grid.touch(b);
        for (s, side) in SIDES.iter().enumerate() {
            let n = b + *side;
            if touch.sides[s] > 0 && seen.contains(&n) && !far.contains_key(&n) {
                far.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }
    (order, far, whole)
}

/// Works out what of `volume` (as it is after some of its voxels were dug
/// out, at `dug`, in voxels from its corner) came loose: takes it out of
/// the volume's changes, given back with what came loose.
pub fn loosen(volume: &VoxelVolume, dug: &[IVec3]) -> (VoxelChanges, VoxelLoosened) {
    let mut changes = (*volume.changes).clone();
    let mut done = VoxelLoosened::default();
    if !volume.endless || volume.anchor == VoxelAnchor::Everything || dug.is_empty() {
        return (changes, done);
    }
    let block = volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
    let woods = trees::woods(&volume.trees, block);
    let mut grid = Grid::new(volume, woods.as_deref());
    let loose = loose_from(&mut grid, dug, &mut done);
    take_out(&mut grid, loose, &mut changes, &mut done);
    (changes, done)
}

/// The sets of built voxels next to `dug` that hold to nothing, each a
/// set of voxels that touch.
pub(super) fn loose_from(grid: &mut Grid, dug: &[IVec3], done: &mut VoxelLoosened) -> Vec<Vec<IVec3>> {
    let mut seeds: Vec<IVec3> = Vec::new();
    let dug_set: HashSet<IVec3> = dug.iter().copied().collect();
    for d in dug {
        for side in SIDES {
            let n = *d + side;
            if !dug_set.contains(&n) && grid.class(n) == BUILT {
                seeds.push(n);
            }
        }
    }
    seeds.sort_by_key(|c| c.to_array());
    seeds.dedup();
    let mut held: HashSet<IVec3> = HashSet::new();
    let mut loose: Vec<Vec<IVec3>> = Vec::new();
    let mut component_of: HashMap<IVec3, usize> = HashMap::new();
    let mut components: Vec<(HashMap<IVec3, u32>, bool)> = Vec::new();
    for seed in seeds {
        if held.contains(&seed) || grid.class(seed) != BUILT {
            continue;
        }
        let brick = seed.div_euclid(IVec3::splat(B));
        // Bricks first: what the seed's bricks are joined to, and how far each is from what holds.
        let c = match component_of.get(&brick) {
            Some(&c) => c,
            None => {
                let (bricks, far, whole) = brick_component(grid, brick, MOST_BRICKS);
                done.bricks += bricks.len();
                components.push((far, whole));
                for b in bricks {
                    component_of.insert(b, components.len() - 1);
                }
                components.len() - 1
            }
        };
        let (far, whole) = &components[c];
        if !whole {
            // Joined to more than a check goes over: held.
            held.insert(seed);
            continue;
        }
        // Then voxels, nearest first by the bricks to what holds; none rooted, every voxel joined comes loose.
        let unrooted = far.is_empty();
        let mut seen: HashSet<IVec3> = HashSet::from([seed]);
        let mut found = vec![seed];
        let mut queue: BinaryHeap<Reverse<(u32, u32, [i32; 3])>> = BinaryHeap::new();
        let distance = |c: IVec3| far.get(&c.div_euclid(IVec3::splat(B))).copied().unwrap_or(u32::MAX / 2);
        queue.push(Reverse((distance(seed), 0, seed.to_array())));
        let mut order = 0u32;
        let mut is_held = false;
        while let Some(Reverse((_, _, c))) = queue.pop() {
            let c = IVec3::from(c);
            for side in SIDES {
                let n = c + side;
                match grid.class(n) {
                    HOLDS => {
                        is_held = true;
                    }
                    BUILT if held.contains(&n) => is_held = true,
                    BUILT if seen.insert(n) => {
                        found.push(n);
                        order += 1;
                        let d = if unrooted { 0 } else { distance(n) };
                        queue.push(Reverse((d, order, n.to_array())));
                    }
                    _ => {}
                }
            }
            if is_held || found.len() > MOST_VOXELS {
                is_held = true;
                break;
            }
        }
        done.voxels += found.len();
        if is_held {
            held.extend(found);
            continue;
        }
        for c in &found {
            grid.take(*c);
        }
        loose.push(found);
    }
    loose
}

/// Takes the voxels of `loose` out of `changes`, each set that touches a
/// piece, or crumbs where it is too few.
pub(super) fn take_out(grid: &mut Grid, loose: Vec<Vec<IVec3>>, changes: &mut VoxelChanges, done: &mut VoxelLoosened) {
    let voxel = grid.voxel;
    for mut found in loose {
        done.loose += found.len();
        for c in &found {
            changes.set(*c, None);
        }
        if found.len() < CRUMBS {
            let mut bricks: Vec<IVec3> = found.iter().map(|c| c.div_euclid(IVec3::splat(B))).collect();
            bricks.sort_by_key(|b| b.to_array());
            bricks.dedup();
            done.crumbs.extend(bricks.into_iter().map(|b| (b * B).as_vec3() * voxel + voxel * B as f32 * 0.5));
            continue;
        }
        found.sort_by_key(|c| [c.z, c.y, c.x]);
        done.pieces.push(piece_of(grid, &found));
    }
}

/// The piece of built voxels `found`, in their looks.
pub(super) fn piece_of(grid: &mut Grid, found: &[IVec3]) -> VoxelPiece {
    let voxel = grid.voxel;
    let (least, most) = found.iter().fold((IVec3::MAX, IVec3::MIN), |(l, m), c| (l.min(*c), m.max(*c)));
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut places: HashMap<u16, u8> = HashMap::new();
    let mut mass = 0.0;
    let each = voxel.powi(3);
    let voxels = found
        .iter()
        .map(|c| {
            let look = grid.look_of(*c);
            let kept = grid.looks.get(usize::from(look)).copied();
            mass += kept.map_or(VoxelStuff::Stone, |look| look.stuff).density() * each;
            let place = *places.entry(look).or_insert_with(|| {
                palette.push(kept.map_or([160, 150, 140, 255], |look| look.colour));
                palette.len().min(255) as u8
            });
            ((*c - least).to_array().map(|a| a as u16), place)
        })
        .collect();
    let size = (most - least + 1).to_array().map(|a| a as u32);
    let wood = vec![true; palette.len()];
    VoxelPiece { low: least, size, voxels, palette, wood, voxel, mass }
}

/// Where a blast goes: every built voxel whose middle is within `radius`
/// metres of `at`, in metres from the volume's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelBlast {
    pub at: Vec3,
    pub radius: f32,
}

/// What settling after a change did: what came loose at once, cut off
/// from what holds, and what gave way under its own weight after, round
/// by round.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelSettled {
    pub loosened: VoxelLoosened,
    pub strain: VoxelStrainDone,
}

/// What a blast did: how many built voxels it dug out, where (a place in
/// metres for each brick of them, for the dust they make), and what came
/// loose after.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelBlastDone {
    pub dug: usize,
    pub dust: Vec<Vec3>,
    pub settled: VoxelSettled,
}

/// Works out what came loose of what was built after `before` became
/// `after` by voxels at `dug` (in voxels from the volume's corner) dug
/// out: what was cut off from what holds, taken out of the changes given
/// back, and what gives way after under its own weight, as `after`'s
/// strain says, `memory` keeping what each join bore at rest. What gives
/// way after is left in the changes, for the game to dig out round by
/// round.
pub fn settle(
    before: &VoxelVolume,
    after: &VoxelVolume,
    dug: &[IVec3],
    memory: &mut VoxelStrainMemory,
) -> (VoxelChanges, VoxelSettled) {
    let mut changes = (*after.changes).clone();
    let mut done = VoxelSettled::default();
    if !after.endless || after.anchor == VoxelAnchor::Everything || dug.is_empty() {
        return (changes, done);
    }
    let block = after.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
    let woods = trees::woods(&after.trees, block);
    let mut grid = Grid::new(after, woods.as_deref());
    let loose = loose_from(&mut grid, dug, &mut done.loosened);
    take_out(&mut grid, loose, &mut changes, &mut done.loosened);
    if after.strain != VoxelStrain::Off {
        let mut was = Grid::new(before, woods.as_deref());
        done.strain = strain(after.strain, &mut was, &mut grid, dug, memory);
    }
    (changes, done)
}

/// Digs out what was built of `volume` in `blast`, and settles what is
/// left ([`settle`]): gives the volume's changes after it, and what it
/// did. The ground, and what holds as the ground does, is left.
pub fn blast(
    volume: &VoxelVolume,
    blast: VoxelBlast,
    memory: &mut VoxelStrainMemory,
) -> (VoxelChanges, VoxelBlastDone) {
    let mut done = VoxelBlastDone::default();
    if !volume.endless || blast.radius <= 0.0 {
        return ((*volume.changes).clone(), done);
    }
    let block = volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
    let woods = trees::woods(&volume.trees, block);
    let mut changes = (*volume.changes).clone();
    // What it digs, worked out on the volume as it was.
    let dug: Vec<IVec3> = {
        let mut grid = Grid::new(volume, woods.as_deref());
        let voxel = grid.voxel;
        let (low, high) = (
            ((blast.at - blast.radius) / voxel).floor().as_ivec3(),
            ((blast.at + blast.radius) / voxel).floor().as_ivec3(),
        );
        let mut dug = Vec::new();
        for z in low.z..=high.z {
            for y in low.y.max(1)..=high.y {
                for x in low.x..=high.x {
                    let c = IVec3::new(x, y, z);
                    if ((c.as_vec3() + 0.5) * voxel).distance(blast.at) <= blast.radius && grid.class(c) == BUILT {
                        dug.push(c);
                    }
                }
            }
        }
        dug
    };
    if dug.is_empty() {
        return (changes, done);
    }
    for c in &dug {
        changes.set(*c, None);
    }
    let voxel = 1.0 / block as f32;
    let mut bricks: Vec<IVec3> = dug.iter().map(|c| c.div_euclid(IVec3::splat(B))).collect();
    bricks.sort_by_key(|b| b.to_array());
    bricks.dedup();
    done.dust = bricks.into_iter().map(|b| (b * B).as_vec3() * voxel + voxel * B as f32 * 0.5).collect();
    done.dug = dug.len();
    let after = VoxelVolume { changes: Arc::new(changes), ..volume.clone() };
    let (changes, settled) = settle(volume, &after, &dug, memory);
    done.settled = settled;
    (changes, done)
}

#[cfg(test)]
pub(super) mod tests {
    use std::time::Instant;

    use voxear_core::math::Quat;

    use super::*;
    use crate::{VoxelEdit, VoxelGround, VoxelSolids};

    /// Flat ground a metre up with nothing on it.
    pub fn flat() -> VoxelVolume {
        let ground = VoxelGround { height: 1.0, hills: 0.0, grass: 0.0, ..VoxelGround::default() };
        VoxelVolume { endless: true, size: Vec3::new(16.0, 32.0, 16.0), ground: Some(ground), ..VoxelVolume::default() }
    }

    /// A house of bricks on `volume`'s ground, its floor's corner at `at`:
    /// walls a quarter of a metre thick round a room `w` by `d` metres and
    /// `h` tall, a door, and a flat roof of planks.
    pub fn house(volume: &mut VoxelVolume, at: Vec3, w: f32, d: f32, h: f32) {
        let edits = Arc::make_mut(&mut volume.edits);
        let middle = at + Vec3::new(w * 0.5, 0.0, d * 0.5);
        edits.push(VoxelEdit::boxed(middle, Vec3::new(w, h, d), Quat::IDENTITY, Some(VoxelMaterial::Bricks)));
        edits.push(VoxelEdit::boxed(middle, Vec3::new(w - 0.5, h, d - 0.5), Quat::IDENTITY, None));
        edits.push(VoxelEdit::boxed(at + Vec3::new(w * 0.5, 0.0, 0.0), Vec3::new(1.0, 2.0, 0.6), Quat::IDENTITY, None));
        let roof = middle + Vec3::Y * h;
        edits.push(VoxelEdit::boxed(
            roof,
            Vec3::new(w + 0.5, 0.25, d + 0.5),
            Quat::IDENTITY,
            Some(VoxelMaterial::Planks),
        ));
    }

    #[test]
    fn what_is_built_is_solid_where_collision_finds_it_and_the_ground_holds_it() {
        let mut volume = flat();
        house(&mut volume, Vec3::new(4.0, 1.0, 4.0), 4.0, 3.0, 2.5);
        Arc::make_mut(&mut volume.edits).push(VoxelEdit::ball(
            Vec3::new(10.0, 1.5, 10.0),
            1.0,
            Some(VoxelMaterial::Dirt),
        ));
        let solids = VoxelSolids::of(&volume).unwrap();
        let mut grid = Grid::new(&volume, None);
        let (low, size) = ([60, 8, 60], [48, 48, 48]);
        let mut solid = vec![false; 48 * 48 * 48];
        solids.fill(low, size, &mut solid);
        let (mut built, mut holds) = (0, 0);
        for z in 0..48 {
            for y in 0..48 {
                for x in 0..48 {
                    let class = grid.class(IVec3::new(low[0] + x, low[1] + y, low[2] + z));
                    assert_eq!(class != AIR, solid[(x + 48 * (y + 48 * z)) as usize], "{x} {y} {z}");
                    built += usize::from(class == BUILT);
                    holds += usize::from(class == HOLDS);
                }
            }
        }
        assert!(built > 10_000 && holds > 10_000, "{built} built, {holds} holding");
        // The ball of dirt holds as the ground does; with the ground alone holding, it is built.
        assert_eq!(grid.class(IVec3::new(160, 30, 160)), HOLDS);
        let only = VoxelVolume { anchor: VoxelAnchor::Ground, ..volume.clone() };
        assert_eq!(Grid::new(&only, None).class(IVec3::new(160, 30, 160)), BUILT);
    }

    #[test]
    fn a_wall_over_a_breach_holds_by_its_sides_and_comes_loose_once_they_are_cut() {
        let mut volume = flat();
        // A wall 4 m long, 2 m tall, a quarter metre thick, standing on the ground.
        let wall = VoxelEdit::boxed(
            Vec3::new(8.0, 1.0, 8.0),
            Vec3::new(4.0, 2.0, 0.25),
            Quat::IDENTITY,
            Some(VoxelMaterial::Ashlar),
        );
        Arc::make_mut(&mut volume.edits).push(wall);
        // A breach through its foot, 2 m wide and half a metre tall: the wall over it holds by its sides.
        let mut changes = (*volume.changes).clone();
        let mut dug = Vec::new();
        for z in 124..132 {
            for y in 16..24 {
                for x in 112..144 {
                    changes.set(IVec3::new(x, y, z), None);
                    dug.push(IVec3::new(x, y, z));
                }
            }
        }
        volume.changes = Arc::new(changes);
        let (_, done) = loosen(&volume, &dug);
        assert!(done.pieces.is_empty() && done.loose == 0, "{} loose", done.loose);
        // Its sides cut through to the breach: what is over the breach comes loose, one piece.
        let mut changes = (*volume.changes).clone();
        let mut cut = Vec::new();
        for z in 124..132 {
            for y in 16..48 {
                for x in [110, 111, 144, 145] {
                    changes.set(IVec3::new(x, y, z), None);
                    cut.push(IVec3::new(x, y, z));
                }
            }
        }
        volume.changes = Arc::new(changes);
        let started = Instant::now();
        let (after, done) = loosen(&volume, &cut);
        let took = started.elapsed();
        assert_eq!(done.pieces.len(), 1, "{:?}", done.pieces.iter().map(|p| p.voxels.len()).collect::<Vec<_>>());
        let piece = &done.pieces[0];
        assert_eq!(piece.voxels.len(), 32 * 24 * 4, "the wall over the breach, between the cuts");
        assert_eq!(piece.low, IVec3::new(112, 24, 126));
        assert!(piece.mass > 100.0 && piece.palette.len() >= 2, "{} kg", piece.mass);
        eprintln!("{} bricks, {} voxels gone over in {took:?}", done.bricks, done.voxels);
        // Taken out of the volume: collision finds it open.
        let after = VoxelVolume { changes: Arc::new(after), ..volume };
        let solids = VoxelSolids::of(&after).unwrap();
        let mut solid = vec![false; 32 * 24 * 4];
        solids.fill([112, 24, 126], [32, 24, 4], &mut solid);
        assert!(solid.iter().all(|s| !s));
    }

    #[test]
    fn a_balcony_cut_from_its_house_comes_loose_and_the_house_stands() {
        let mut volume = flat();
        house(&mut volume, Vec3::new(4.0, 1.0, 4.0), 4.0, 3.0, 2.5);
        // A balcony out of the house's side, a metre out, at the top of the wall.
        let balcony = VoxelEdit::boxed(
            Vec3::new(9.0, 2.75, 5.5),
            Vec3::new(2.0, 0.25, 1.0),
            Quat::IDENTITY,
            Some(VoxelMaterial::Planks),
        );
        Arc::make_mut(&mut volume.edits).push(balcony);
        // Cut where it meets the wall: a slab of voxels, x from 8 m to 8.25 m.
        let mut changes = (*volume.changes).clone();
        let mut dug = Vec::new();
        for z in 80..96 {
            for y in 44..48 {
                for x in 128..132 {
                    let c = IVec3::new(x, y, z);
                    changes.set(c, None);
                    dug.push(c);
                }
            }
        }
        volume.changes = Arc::new(changes);
        let (_, done) = loosen(&volume, &dug);
        assert_eq!(done.pieces.len(), 1, "{done:?}");
        let piece = &done.pieces[0];
        assert!(piece.low.x >= 132 && piece.voxels.len() == 28 * 4 * 16, "{:?} {}", piece.low, piece.voxels.len());
        assert!(done.loose == piece.voxels.len(), "only the balcony");
    }

    #[test]
    fn a_blast_through_a_bridges_pier_drops_its_span_and_crumbs_are_not_pieces() {
        let mut volume = flat();
        let edits = Arc::make_mut(&mut volume.edits);
        // Two piers and a span over them.
        for x in [4.0, 10.0] {
            edits.push(VoxelEdit::boxed(
                Vec3::new(x, 1.0, 8.0),
                Vec3::new(0.5, 2.0, 1.0),
                Quat::IDENTITY,
                Some(VoxelMaterial::Ashlar),
            ));
        }
        edits.push(VoxelEdit::boxed(
            Vec3::new(7.0, 3.0, 8.0),
            Vec3::new(7.0, 0.25, 1.0),
            Quat::IDENTITY,
            Some(VoxelMaterial::Cobbles),
        ));
        // One pier blown through: the span still hangs on the other.
        let mut memory = VoxelStrainMemory::default();
        volume.strain = VoxelStrain::Off;
        let (changes, done) = blast(&volume, VoxelBlast { at: Vec3::new(10.0, 2.0, 8.0), radius: 0.9 }, &mut memory);
        assert!(done.dug > 1000 && !done.dust.is_empty(), "{} dug", done.dug);
        assert!(
            done.settled.loosened.pieces.is_empty(),
            "held by the other pier: {:?}",
            done.settled.loosened.pieces.len()
        );
        let volume = VoxelVolume { changes: Arc::new(changes), ..volume };
        // The other too: the span comes loose with what is left of the piers' tops.
        let (_, done) = blast(&volume, VoxelBlast { at: Vec3::new(4.0, 2.0, 8.0), radius: 0.9 }, &mut memory);
        let pieces = &done.settled.loosened.pieces;
        assert!(!pieces.is_empty(), "{done:?}");
        let biggest = pieces.iter().map(|p| p.voxels.len()).max().unwrap();
        assert!(biggest >= 7 * 16 * 4 * 16 - 2000, "the span: {biggest}");
        assert!(pieces.iter().all(|p| p.voxels.len() >= CRUMBS));
    }
}
