//! The changes made to an endless Voxel Volume ([`VoxelEdit`]), kept in
//! order, a later one over an earlier: balls, boxes, banks and outlined
//! areas carved out or filled with a ground or a paving, the top of the
//! ground within an outline paved, and Voxel Prints printed.
//!
//! The GPU works each out a voxel at a time (`editHeld` in
//! `shaders/voxel_world.slang`), and the CPU the same way here
//! ([`VoxelEdit::held`]), so collision and rays find what is drawn.

use std::sync::Arc;

use voxear_core::math::{IVec3, Mat3, Quat, Vec3};

use super::VoxelPrint;

/// What an edit fills with: a ground, in the shades the volume's own
/// ground or a generator's has (dirt; grass, its top the grass's green;
/// rock; sand; snow; gravel), or a paving laid in its own pattern
/// (cobbles, flagstones, bricks, planks, or dressed stone blocks).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelMaterial {
    #[default]
    Dirt,
    Grass,
    Rock,
    Sand,
    Snow,
    Gravel,
    Cobbles,
    Flagstones,
    Bricks,
    Planks,
    Ashlar,
}

impl VoxelMaterial {
    pub const ALL: [Self; 11] = [
        Self::Dirt,
        Self::Grass,
        Self::Rock,
        Self::Sand,
        Self::Snow,
        Self::Gravel,
        Self::Cobbles,
        Self::Flagstones,
        Self::Bricks,
        Self::Planks,
        Self::Ashlar,
    ];

    /// As the editor and scenes name it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dirt => "Dirt",
            Self::Grass => "Grass",
            Self::Rock => "Rock",
            Self::Sand => "Sand",
            Self::Snow => "Snow",
            Self::Gravel => "Gravel",
            Self::Cobbles => "Cobbles",
            Self::Flagstones => "Flagstones",
            Self::Bricks => "Bricks",
            Self::Planks => "Planks",
            Self::Ashlar => "Ashlar",
        }
    }

    /// The one named so, as [`name`](Self::name) gives it.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|material| material.name().eq_ignore_ascii_case(name))
    }

    /// Whether it is a paving, laid in a pattern of its own.
    pub fn is_paving(self) -> bool {
        (self as usize) >= Self::Cobbles as usize
    }
}

/// The colours of the pavings, as sRGB, in the palette after the ground a
/// generator makes: for each, three shades of its stones, bricks or boards
/// and the colour of the joints between them (`fillVoxel` in
/// `shaders/voxel_world.slang` lays them out).
pub const PAVING_COLORS: [[u8; 4]; 20] = [
    [128, 124, 116, 255], // cobbles
    [146, 140, 130, 255],
    [110, 106, 100, 255],
    [84, 80, 74, 255],
    [168, 160, 144, 255], // flagstones
    [184, 176, 160, 255],
    [154, 146, 132, 255],
    [104, 98, 90, 255],
    [150, 72, 52, 255], // bricks
    [168, 86, 62, 255],
    [132, 62, 46, 255],
    [196, 188, 172, 255],
    [132, 96, 62, 255], // planks
    [148, 110, 72, 255],
    [118, 84, 54, 255],
    [74, 54, 38, 255],
    [196, 186, 162, 255], // ashlar
    [208, 200, 178, 255],
    [182, 172, 150, 255],
    [150, 142, 124, 255],
];

/// The shape of an edit, in metres from the volume's corner.
#[derive(Clone, Debug, PartialEq)]
pub enum VoxelShape {
    /// A ball, its voxels' middles within `radius` of `at`.
    Ball { at: Vec3, radius: f32 },
    /// A box `size` metres along its own X, Y and Z, the middle of its foot
    /// at `at`, turned `rotation` about there.
    Box { at: Vec3, size: Vec3, rotation: Quat },
    /// A bank: a box whose top slopes from its foot at its -Z end up to its
    /// height at its +Z end, as a box is placed and turned.
    Ramp { at: Vec3, size: Vec3, rotation: Quat },
    /// An outline across the ground, its points along X and Z, from `low`
    /// up to `high` metres: a street, a square, a quay, a plot.
    Area { points: Arc<Vec<[f32; 2]>>, low: f32, high: f32 },
    /// The top `depth` metres of the ground within an outline, wherever its
    /// ground is: a road paved over hills, a field laid.
    Cover { points: Arc<Vec<[f32; 2]>>, depth: f32 },
    /// A Voxel Print printed.
    Print(VoxelPrinted),
}

/// A Voxel Print as an edit prints it: the print, where its model's origin
/// is, how it is turned about there, and whether it is fitted to the ground.
#[derive(Clone, Debug)]
pub struct VoxelPrinted {
    pub print: Arc<VoxelPrint>,
    pub at: Vec3,
    pub rotation: Quat,
    /// Fitted, the ground inside it is cleared down to its foot, and where
    /// the ground falls away under it a footing of the edit's fill reaches
    /// down to the ground, as deep as the print is tall at most.
    pub fit: bool,
}

impl PartialEq for VoxelPrinted {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.print, &other.print) || self.print == other.print)
            && self.at == other.at
            && self.rotation == other.rotation
            && self.fit == other.fit
    }
}

/// A change made to an endless Voxel Volume: a shape carved out of it or
/// filled, or a Voxel Print printed into it.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelEdit {
    pub shape: VoxelShape,
    /// What fills it: none for air, carved out. A print takes its own
    /// colours; fitted, its footing is of its fill.
    pub fill: Option<VoxelMaterial>,
    /// The colour its fill's or print's colours take, the middle of them
    /// made this colour and the others shaded as they were, as sRGB: none
    /// for their own.
    pub tint: Option<[u8; 3]>,
    /// How far over its top a box, a bank, an area or a cover clears the
    /// ground and anything in it, in metres: the air over a street or a
    /// levelled plot.
    pub clear: f32,
}

/// What an edit makes of a voxel ([`VoxelEdit::held`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelHeld {
    /// It does not reach the voxel: what was there stays.
    Not,
    /// Carved out, or cleared.
    Air,
    /// Filled with a ground or a paving.
    Fill(VoxelMaterial),
    /// A print's voxel, its colour as a place in the print's palette.
    Colour(u8),
}

impl VoxelHeld {
    /// Whether it leaves the voxel solid.
    pub fn solid(self) -> bool {
        matches!(self, Self::Fill(_) | Self::Colour(_))
    }
}

/// How far an edit with no height of its own (a cover) reaches up and down,
/// in metres: all there is.
const ALL_THE_WAY: f32 = 1.0e5;

impl VoxelEdit {
    fn of(shape: VoxelShape, fill: Option<VoxelMaterial>) -> Self {
        Self { shape, fill, tint: None, clear: 0.0 }
    }

    /// A ball `radius` metres round `at`, carved out, or filled with `fill`.
    pub fn ball(at: Vec3, radius: f32, fill: Option<VoxelMaterial>) -> Self {
        Self::of(VoxelShape::Ball { at, radius: radius.max(0.0) }, fill)
    }

    /// A box `size` metres, the middle of its foot at `at`, turned
    /// `rotation`, carved out or filled with `fill`.
    pub fn boxed(at: Vec3, size: Vec3, rotation: Quat, fill: Option<VoxelMaterial>) -> Self {
        Self::of(VoxelShape::Box { at, size: size.max(Vec3::ZERO), rotation: rotation.normalize() }, fill)
    }

    /// A slab half a metre tall, `width` by `depth` metres along X and Z,
    /// the middle of its foot at `at`.
    pub fn slab(at: Vec3, width: f32, depth: f32, fill: Option<VoxelMaterial>) -> Self {
        Self::boxed(at, Vec3::new(width, 0.5, depth), Quat::IDENTITY, fill)
    }

    /// A bank `size` metres, its top rising from its foot at its -Z end to
    /// its height at its +Z end, placed and turned as a box is.
    pub fn ramp(at: Vec3, size: Vec3, rotation: Quat, fill: Option<VoxelMaterial>) -> Self {
        Self::of(VoxelShape::Ramp { at, size: size.max(Vec3::ZERO), rotation: rotation.normalize() }, fill)
    }

    /// The outline `points` (along X and Z) from `low` up to `high` metres,
    /// carved out or filled with `fill`.
    pub fn area(points: Vec<[f32; 2]>, low: f32, high: f32, fill: Option<VoxelMaterial>) -> Self {
        Self::of(VoxelShape::Area { points: Arc::new(points), low: low.min(high), high: high.max(low) }, fill)
    }

    /// The top `depth` metres of the ground within the outline `points`,
    /// laid with `fill` or, with none, dug away.
    pub fn cover(points: Vec<[f32; 2]>, depth: f32, fill: Option<VoxelMaterial>) -> Self {
        Self::of(VoxelShape::Cover { points: Arc::new(points), depth: depth.max(0.0) }, fill)
    }

    /// `print` printed with its origin at `at`, turned `rotation`.
    pub fn printed(print: Arc<VoxelPrint>, at: Vec3, rotation: Quat) -> Self {
        Self::of(VoxelShape::Print(VoxelPrinted { print, at, rotation: rotation.normalize(), fit: false }), None)
    }

    /// The print fitted to the ground: cleared inside, on a footing of
    /// `footing` where the ground falls away under it (none for no footing).
    pub fn fitted(mut self, footing: Option<VoxelMaterial>) -> Self {
        if let VoxelShape::Print(printed) = &mut self.shape {
            printed.fit = true;
            self.fill = footing;
        }
        self
    }

    /// Its fill or its print in `tint`'s colour.
    pub fn tinted(mut self, tint: Option<[u8; 3]>) -> Self {
        self.tint = tint;
        self
    }

    /// Clearing `metres` over its top.
    pub fn clearing(mut self, metres: f32) -> Self {
        self.clear = metres.max(0.0);
        self
    }

    /// The print it prints, if it is one.
    pub fn print(&self) -> Option<&VoxelPrinted> {
        match &self.shape {
            VoxelShape::Print(printed) => Some(printed),
            _ => None,
        }
    }

    /// Where it is placed: a ball's middle, the middle of a box's, a bank's
    /// or a print's foot, the middle of an outline's box.
    pub fn at(&self) -> Vec3 {
        match &self.shape {
            VoxelShape::Ball { at, .. } | VoxelShape::Box { at, .. } | VoxelShape::Ramp { at, .. } => *at,
            VoxelShape::Print(printed) => printed.at,
            VoxelShape::Area { .. } | VoxelShape::Cover { .. } => {
                let (low, high) = self.reach();
                (low + high) * 0.5
            }
        }
    }

    /// Whether it may leave anything solid: a fill or a print.
    pub fn fills(&self) -> bool {
        self.fill.is_some() || self.print().is_some()
    }

    /// The box it reaches, least and most corners, in metres from the
    /// volume's corner: with what it clears over it, and a fitted print's
    /// footing under it. A cover reaches all the way up and down.
    pub fn reach(&self) -> (Vec3, Vec3) {
        match &self.shape {
            VoxelShape::Ball { at, radius } => (*at - *radius, *at + *radius),
            VoxelShape::Box { at, size, rotation } | VoxelShape::Ramp { at, size, rotation } => {
                let half = Vec3::new(size.x * 0.5, 0.0, size.z * 0.5);
                turned_bounds(*at, *rotation, -half, half + Vec3::Y * (size.y + self.clear))
            }
            VoxelShape::Area { points, low, high } => {
                let (least, most) = outline_bounds(points);
                (Vec3::new(least[0], *low, least[1]), Vec3::new(most[0], high + self.clear, most[1]))
            }
            VoxelShape::Cover { points, .. } => {
                let (least, most) = outline_bounds(points);
                (Vec3::new(least[0], -ALL_THE_WAY, least[1]), Vec3::new(most[0], ALL_THE_WAY, most[1]))
            }
            VoxelShape::Print(printed) => {
                let (mut low, high) = printed.print.bounds();
                if printed.fit {
                    low.y -= high.y - low.y;
                }
                turned_bounds(printed.at, printed.rotation, low, high)
            }
        }
    }

    /// Whether a print is looked at round each voxel's middle too, a
    /// quarter of a voxel toward its eight corners, in a volume of voxels
    /// `voxel` metres across: where it is turned off the quarter turns, or
    /// its voxels are smaller than the volume's, so a wall a voxel thick
    /// leaves no gap between voxels that meet only at their edges, and a
    /// thin pole is not missed. Nothing else is.
    pub fn widened(&self, voxel: f32) -> bool {
        let Some(printed) = self.print() else { return false };
        let turn = Mat3::from_quat(printed.rotation);
        let quarter = turn.to_cols_array().iter().all(|a| a.abs() < 1e-4 || (a.abs() - 1.0).abs() < 1e-4);
        !quarter || printed.print.voxels_per_metre as f32 * voxel > 1.001
    }

    /// What it makes of voxel `cell` of a volume of voxels `voxel` metres
    /// across, whose column's ground has its top at voxel `ground`, worked
    /// out as the GPU works it (`editHeld` in `shaders/voxel_world.slang`):
    /// in whole numbers ([`Fixed`]), so the two find the same to the voxel.
    pub fn held(&self, cell: IVec3, voxel: f32, ground: i32) -> VoxelHeld {
        Fixed::of(self, voxel).held(self, cell, ground)
    }

    /// Whether a print stands over voxel `cell` (`editCovers`): in a column
    /// of its own voxels that has any solid, under its top and no further
    /// below its foot than it is tall. Nothing else stands over anything.
    pub fn covers(&self, cell: IVec3, voxel: f32) -> bool {
        let Some(printed) = self.print() else { return false };
        let c = Fixed::of(self, voxel).print_cell(cell, IVec3::ZERO);
        let tall = printed.print.size[1] as i32;
        c.y < tall && c.y >= -tall && printed.print.stands_over(c.x, c.z)
    }
}

/// How many parts of a voxel an edit's places are worked out in, and of a
/// turn's numbers, as whole numbers: sixteenths of a voxel, and 4096ths.
pub(crate) const UNIT: i64 = 16;
pub(crate) const TURN: i64 = 4096;

/// An edit as whole numbers, which the GPU is given and works with as the
/// CPU does here, in sixteenths of a voxel: where it is placed, how it is
/// turned back (its rows, in 4096ths), a ball's radius or a box's half
/// width, height and half length, what it clears, a print's origin among
/// its own voxels and how many of its voxels and of the volume's make a
/// metre, an outline's points, an area's foot and top; and for a cover its
/// depth and what it clears, in whole voxels.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Fixed {
    pub at: [i64; 3],
    pub back: [[i64; 3]; 3],
    pub size: [i64; 3],
    pub clear: i64,
    pub origin: [i64; 3],
    pub per_metre: [i64; 2],
    pub widened: bool,
    pub points: Vec<[i64; 2]>,
    pub low: i64,
    pub high: i64,
    pub cover: [i32; 2],
}

/// `v` voxels in sixteenths of a voxel, to the nearest.
fn units(v: f32) -> i64 {
    (f64::from(v) * UNIT as f64).round() as i64
}

impl Fixed {
    pub fn of(edit: &VoxelEdit, voxel: f32) -> Self {
        let mut fixed = Fixed { clear: units(edit.clear / voxel), ..Default::default() };
        let turned = |rotation: Quat| {
            let m = Mat3::from_quat(rotation.inverse());
            let cols = [m.x_axis, m.y_axis, m.z_axis];
            [0, 1, 2].map(|i| [0, 1, 2].map(|j| (f64::from(cols[j][i]) * TURN as f64).round() as i64))
        };
        let placed = |at: Vec3| [at.x, at.y, at.z].map(|a| units(a / voxel));
        let outline = |points: &[[f32; 2]]| points.iter().map(|p| [units(p[0] / voxel), units(p[1] / voxel)]).collect();
        match &edit.shape {
            VoxelShape::Ball { at, radius } => {
                fixed.at = placed(*at);
                fixed.size[0] = units(radius / voxel);
            }
            VoxelShape::Box { at, size, rotation } | VoxelShape::Ramp { at, size, rotation } => {
                fixed.at = placed(*at);
                fixed.back = turned(*rotation);
                fixed.size = [units(size.x / voxel * 0.5), units(size.y / voxel), units(size.z / voxel * 0.5)];
            }
            VoxelShape::Area { points, low, high } => {
                fixed.points = outline(points);
                (fixed.low, fixed.high) = (units(low / voxel), units(high / voxel));
            }
            VoxelShape::Cover { points, depth } => {
                fixed.points = outline(points);
                let (depth, clear) = cover_voxels(*depth, edit.clear, voxel);
                fixed.cover = [depth, clear];
            }
            VoxelShape::Print(printed) => {
                fixed.at = placed(printed.at);
                fixed.back = turned(printed.rotation);
                let o = printed.print.origin;
                fixed.origin = [o.x, o.y, o.z].map(units);
                let volume = (1.0 / voxel).round() as i64;
                fixed.per_metre = [i64::from(printed.print.voxels_per_metre.max(1)), volume.max(1)];
                fixed.widened = edit.widened(voxel);
            }
        }
        fixed
    }

    /// The middle of voxel `cell`, moved by `by` sixteenths, from where the
    /// edit is placed and turned back with it, in sixteenths times [`TURN`].
    fn local(&self, cell: IVec3, by: IVec3) -> [i64; 3] {
        let off = [0, 1, 2].map(|a| i64::from(cell[a]) * UNIT + UNIT / 2 + i64::from(by[a]) - self.at[a]);
        [0, 1, 2].map(|i| (0..3).map(|j| self.back[i][j] * off[j]).sum())
    }

    /// The voxel of a print's own that the middle of voxel `cell` of the
    /// volume, moved by `by` sixteenths, is in.
    pub fn print_cell(&self, cell: IVec3, by: IVec3) -> IVec3 {
        let local = self.local(cell, by);
        let [print, volume] = self.per_metre;
        let whole = UNIT * TURN * volume;
        IVec3::from([0, 1, 2].map(|a| (local[a] * print + self.origin[a] * TURN * volume).div_euclid(whole) as i32))
    }

    /// Whether the middle of voxel column (`x`, `z`) is inside the outline,
    /// crossing its edges from it along X an odd number of times, as the GPU
    /// finds it (`insideOutline`).
    fn inside(&self, x: i32, z: i32) -> bool {
        let (px, pz) = (i64::from(x) * UNIT + UNIT / 2, i64::from(z) * UNIT + UNIT / 2);
        let n = self.points.len();
        if n < 3 {
            return false;
        }
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (self.points[i], self.points[j]);
            if (a[1] > pz) != (b[1] > pz) {
                // Left of where the edge crosses: (px - ax)(bz - az) < (bx - ax)(pz - az), the other way round
                // where the edge goes the other way.
                let (left, right) = ((px - a[0]) * (b[1] - a[1]), (b[0] - a[0]) * (pz - a[1]));
                if (b[1] > a[1] && left < right) || (b[1] < a[1] && left > right) {
                    inside = !inside;
                }
            }
            j = i;
        }
        inside
    }

    /// What `edit`, which these are of, makes of voxel `cell`, its column's
    /// ground's top at `ground` ([`VoxelEdit::held`]).
    pub fn held(&self, edit: &VoxelEdit, cell: IVec3, ground: i32) -> VoxelHeld {
        let filled = match edit.fill {
            Some(material) => VoxelHeld::Fill(material),
            None => VoxelHeld::Air,
        };
        match &edit.shape {
            VoxelShape::Ball { .. } => {
                let off = [0, 1, 2].map(|a| i64::from(cell[a]) * UNIT + UNIT / 2 - self.at[a]);
                let far = off.iter().map(|o| o * o).sum::<i64>();
                if far <= self.size[0] * self.size[0] { filled } else { VoxelHeld::Not }
            }
            VoxelShape::Box { .. } | VoxelShape::Ramp { .. } => {
                let [x, y, z] = self.local(cell, IVec3::ZERO);
                let [hx, hy, hz] = self.size.map(|v| v * TURN);
                if x.abs() >= hx || z.abs() >= hz || y < 0 {
                    return VoxelHeld::Not;
                }
                let clear = self.clear * TURN;
                // Under the top: for a bank, rising from its foot at -Z to its height at +Z.
                let ramp = matches!(edit.shape, VoxelShape::Ramp { .. });
                let under = |by: i64| if ramp { (y - by) * (2 * hz) < hy * (z + hz) } else { y - by < hy };
                if under(0) {
                    filled
                } else if under(clear) {
                    VoxelHeld::Air
                } else {
                    VoxelHeld::Not
                }
            }
            VoxelShape::Area { .. } => {
                if !self.inside(cell.x, cell.z) {
                    return VoxelHeld::Not;
                }
                let y = i64::from(cell.y) * UNIT + UNIT / 2;
                if y >= self.low && y < self.high {
                    filled
                } else if y >= self.high && y < self.high + self.clear {
                    VoxelHeld::Air
                } else {
                    VoxelHeld::Not
                }
            }
            VoxelShape::Cover { .. } => {
                if !self.inside(cell.x, cell.z) {
                    return VoxelHeld::Not;
                }
                let [depth, clear] = self.cover;
                if cell.y <= ground && cell.y > ground - depth {
                    filled
                } else if cell.y > ground && cell.y <= ground + clear {
                    VoxelHeld::Air
                } else {
                    VoxelHeld::Not
                }
            }
            VoxelShape::Print(printed) => self.printed_held(edit, printed, cell, ground),
        }
    }

    fn printed_held(&self, edit: &VoxelEdit, printed: &VoxelPrinted, cell: IVec3, ground: i32) -> VoxelHeld {
        let print = &printed.print;
        let c = self.print_cell(cell, IVec3::ZERO);
        let held = print.get(c.to_array());
        if held != 0 {
            return VoxelHeld::Colour(held);
        }
        if self.widened {
            // A quarter of a voxel toward each of its corners.
            let corners = (0..8).map(|k| IVec3::new(k & 1, k >> 1 & 1, k >> 2 & 1) * 8 - 4);
            if let Some(held) = corners.map(|by| print.get(self.print_cell(cell, by).to_array())).find(|&h| h != 0) {
                return VoxelHeld::Colour(held);
            }
        }
        let tall = print.size[1] as i32;
        if !printed.fit || c.y >= tall || c.y < -tall || !print.stands_over(c.x, c.z) {
            return VoxelHeld::Not;
        }
        if c.y >= 0 {
            return VoxelHeld::Air;
        }
        match edit.fill {
            Some(material) if cell.y >= ground - 1 => VoxelHeld::Fill(material),
            _ => VoxelHeld::Not,
        }
    }
}

/// A cover's depth and what it clears over the ground, in whole voxels: at
/// least a voxel deep.
pub(crate) fn cover_voxels(depth: f32, clear: f32, voxel: f32) -> (i32, i32) {
    (((depth / voxel).round() as i32).max(1), (clear / voxel).round() as i32)
}

/// The box a box from `low` to `high` reaches, turned `rotation` and placed
/// at `at`.
fn turned_bounds(at: Vec3, rotation: Quat, low: Vec3, high: Vec3) -> (Vec3, Vec3) {
    let mut least = Vec3::splat(f32::MAX);
    let mut most = Vec3::splat(f32::MIN);
    for corner in 0..8 {
        let c = Vec3::new(
            if corner & 1 == 0 { low.x } else { high.x },
            if corner & 2 == 0 { low.y } else { high.y },
            if corner & 4 == 0 { low.z } else { high.z },
        );
        let p = at + rotation * c;
        least = least.min(p);
        most = most.max(p);
    }
    (least, most)
}

/// An outline's least and most corners along X and Z.
fn outline_bounds(points: &[[f32; 2]]) -> ([f32; 2], [f32; 2]) {
    if points.is_empty() {
        return ([0.0; 2], [0.0; 2]);
    }
    let mut least = [f32::MAX; 2];
    let mut most = [f32::MIN; 2];
    for p in points {
        for a in 0..2 {
            least[a] = least[a].min(p[a]);
            most[a] = most[a].max(p[a]);
        }
    }
    (least, most)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOXEL: f32 = 1.0 / 16.0;

    fn count(edit: &VoxelEdit, low: IVec3, high: IVec3, ground: i32, want: fn(VoxelHeld) -> bool) -> usize {
        let mut found = 0;
        for z in low.z..high.z {
            for y in low.y..high.y {
                for x in low.x..high.x {
                    found += usize::from(want(edit.held(IVec3::new(x, y, z), VOXEL, ground)));
                }
            }
        }
        found
    }

    #[test]
    fn a_box_holds_its_voxels_and_clears_over_its_top() {
        // Two metres by half a metre tall by a metre, its foot's middle at (4, 1, 4), clearing a metre over it.
        let edit =
            VoxelEdit::boxed(Vec3::new(4.0, 1.0, 4.0), Vec3::new(2.0, 0.5, 1.0), Quat::IDENTITY, Some(VoxelMaterial::Cobbles))
                .clearing(1.0);
        let (low, high) = edit.reach();
        assert_eq!((low, high), (Vec3::new(3.0, 1.0, 3.5), Vec3::new(5.0, 2.5, 4.5)));
        let all = (IVec3::new(32, 0, 40), IVec3::new(96, 64, 88));
        assert_eq!(count(&edit, all.0, all.1, 0, |h| h == VoxelHeld::Fill(VoxelMaterial::Cobbles)), 32 * 8 * 16);
        assert_eq!(count(&edit, all.0, all.1, 0, |h| h == VoxelHeld::Air), 32 * 16 * 16);
        // Turned a quarter about Y, it is a metre along X and two along Z.
        let turned = VoxelEdit::boxed(
            Vec3::new(4.0, 1.0, 4.0),
            Vec3::new(2.0, 0.5, 1.0),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            None,
        );
        let (low, high) = turned.reach();
        assert!((high - low - Vec3::new(1.0, 0.5, 2.0)).abs().max_element() < 1e-4, "{low} {high}");
        assert_eq!(count(&turned, all.0, all.1, 0, |h| h == VoxelHeld::Air), 32 * 8 * 16);
    }

    #[test]
    fn a_bank_rises_along_its_length_and_a_slab_is_half_a_metre() {
        let bank = VoxelEdit::ramp(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 4.0), Quat::IDENTITY, Some(VoxelMaterial::Dirt));
        let tall_at = |z: i32| (0..40).filter(|&y| bank.held(IVec3::new(0, y, z), VOXEL, 0).solid()).count();
        // Along its 64 voxels its top rises from nothing to 32 voxels.
        assert!(tall_at(-32) <= 1 && tall_at(31) >= 31 && tall_at(0) == 16, "{} {} {}", tall_at(-32), tall_at(0), tall_at(31));
        let slab = VoxelEdit::slab(Vec3::new(1.0, 2.0, 1.0), 1.0, 1.0, Some(VoxelMaterial::Flagstones));
        let (low, high) = slab.reach();
        assert_eq!(high.y - low.y, 0.5);
        assert_eq!(count(&slab, IVec3::new(0, 0, 0), IVec3::new(32, 64, 32), 0, VoxelHeld::solid), 16 * 8 * 16);
    }

    #[test]
    fn an_area_holds_what_its_outline_does_and_a_cover_follows_the_ground() {
        // An L of three metre squares.
        let outline = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]];
        let area = VoxelEdit::area(outline.clone(), 1.0, 1.5, Some(VoxelMaterial::Planks));
        let solid = count(&area, IVec3::new(-4, 0, -4), IVec3::new(40, 40, 40), 0, VoxelHeld::solid);
        assert_eq!(solid, 3 * 16 * 16 * 8);
        assert_eq!(area.held(IVec3::new(24, 20, 24), VOXEL, 0), VoxelHeld::Not, "the L's missing square");
        // A cover two voxels deep under ground whose top is at 10 here and 30 there.
        let cover = VoxelEdit::cover(outline, 2.0 / 16.0, Some(VoxelMaterial::Cobbles)).clearing(1.0 / 16.0);
        for ground in [10, 30] {
            let at = |y| cover.held(IVec3::new(4, y, 4), VOXEL, ground);
            assert_eq!([at(ground - 2), at(ground - 1), at(ground), at(ground + 1), at(ground + 2)], [
                VoxelHeld::Not,
                VoxelHeld::Fill(VoxelMaterial::Cobbles),
                VoxelHeld::Fill(VoxelMaterial::Cobbles),
                VoxelHeld::Air,
                VoxelHeld::Not
            ]);
        }
    }

    #[test]
    fn a_fitted_print_clears_the_ground_inside_it_and_stands_on_a_footing() {
        // A hut 2 m a side and 2 m tall, a voxel's shell, its origin at its foot's middle.
        let hut = VoxelPrint::from_voxels([32, 32, 32], 16, Vec3::new(16.0, 0.0, 16.0), vec![[200; 4]], Default::default(), |c| {
            u8::from(c[0] == 0 || c[0] == 31 || c[2] == 0 || c[2] == 31 || c[1] == 31)
        });
        let edit = VoxelEdit::printed(Arc::new(hut), Vec3::new(4.0, 2.0, 4.0), Quat::IDENTITY);
        let middle = IVec3::new(64, 40, 64);
        // The ground's top 4 voxels over the hut's foot (at 32): plain, its inside is left as it is.
        assert_eq!(edit.held(middle, VOXEL, 36), VoxelHeld::Not);
        let fitted = edit.clone().fitted(Some(VoxelMaterial::Ashlar));
        assert_eq!(fitted.held(middle, VOXEL, 36), VoxelHeld::Air, "cleared inside");
        assert_eq!(fitted.held(IVec3::new(64, 33, 64), VOXEL, 36), VoxelHeld::Air);
        assert_eq!(fitted.held(IVec3::new(64, 31, 64), VOXEL, 36), VoxelHeld::Not, "the ground under its foot");
        // Where the ground is 10 voxels under its foot, the footing reaches down to it, and no further.
        assert_eq!(fitted.held(IVec3::new(64, 23, 64), VOXEL, 22), VoxelHeld::Fill(VoxelMaterial::Ashlar));
        assert_eq!(fitted.held(IVec3::new(64, 20, 64), VOXEL, 22), VoxelHeld::Not);
        // Its walls keep their colour; outside its footprint nothing changes.
        assert_eq!(fitted.held(IVec3::new(64 - 16, 40, 64), VOXEL, 36), VoxelHeld::Colour(1));
        assert_eq!(fitted.held(IVec3::new(64 - 20, 33, 64), VOXEL, 36), VoxelHeld::Not);
        let (low, _) = fitted.reach();
        assert_eq!(low.y, 0.0, "it reaches down as far as it is tall");
    }
}
