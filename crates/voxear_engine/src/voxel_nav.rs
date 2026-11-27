//! Navigation over endless Voxel Volumes' ground: the ways Nav Agents and
//! Block Mobs walk there (see `voxel_ways`).
//!
//! A volume's voxels are gathered into cells about a quarter of a metre a
//! side (four voxels at sixteen a metre), a cell solid where any voxel in
//! it is or within a quarter of a metre beside it, and the cells are
//! walked as a world of blocks is: places to stand made chunk by chunk,
//! regions over them, ways found and pulled tight (see `voxear_nav`). The ground is solid where collision finds it,
//! its trees' trunks and branches in the way and their leaves, grass and
//! flowers walked through; water over the sea's level or a river's bed is
//! waded or, too deep, swum or kept out of.
//!
//! Cells are made only round what walks: 24 m about each agent, and along
//! each way asked for, on threads of their own, a column of chunks at a
//! time, nearest first. A way asked for before its cells are made goes as
//! far as they reach, and on from there once more are made. A change to
//! the volume makes its cells again where the change reaches; cells far
//! from every agent and not asked for a while are let go.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_nav::{
    BlockNav, BlockNavBuilder, PathStatus, Walker,
    blocks::search::{PathAsk, find_path},
};
use voxear_render::VoxelVolume;
use voxear_scene::{DimensionId, Transform, WorldTransform, dimension_of};
use voxear_terrain::blocks::{
    BlockFaces, BlockMap,
    chunk::{CHUNK, Chunk},
};

use crate::{
    physics::VoxelSolids,
    voxel_ways::{VoxelWay, VoxelWays},
};

/// How large a cell is, in metres, near enough: a whole number of voxels.
const CELL: f32 = 0.25;
/// What a cell holds: nothing, ground, or water.
const AIR: u16 = 0;
const GROUND: u16 = 1;
const WATER: u16 = 2;
/// How far round each agent its cells are made, in metres, and how far
/// beyond that and how long after they were last wanted they are let go.
const AROUND: f32 = 24.0;
const LET_GO: f32 = 48.0;
const UNUSED: Duration = Duration::from_secs(30);
/// How far along a way asked for its cells are made, in metres.
const ALONG: f32 = 96.0;
/// How many columns of chunks are made at once.
const WORKERS: usize = 3;
/// The most chunks a column is made of over its lowest ground.
const MOST_UP: i32 = 12;
/// How far round its voxels a cell is solid, across, in metres: so ways
/// keep that much further from walls and trunks than a cell's middle does.
const CLEAR: f32 = 0.25;
/// How far a step up and a drop may go, in metres, for what walks there.
const STEP: f32 = 0.45;
const DROP: f32 = 2.0;
/// The most places a search looks at, and how many chunks of a long way
/// are found place by place before the rest is found as it is walked.
const LOOKED: usize = 60_000;
const REFINED: u32 = 10;

fn faces() -> Arc<[BlockFaces]> {
    Arc::from([BlockFaces::new(0, 0, 0), BlockFaces::new(0, 0, 0), BlockFaces::water(0)].as_slice())
}

/// A column of chunks made: its cells' chunks, by key.
struct Made {
    column: [i32; 2],
    chunks: Vec<([i32; 3], Chunk)>,
}

/// One endless volume's navigation.
struct Ground {
    solids: Arc<dyn VoxelSolids>,
    corner: Vec3,
    dimension: DimensionId,
    /// Voxels a cell, and metres.
    cell: i32,
    size: f32,
    /// Where the volume's water stands, for its cells: the volume as it is
    /// now, its corner, and its sea's height in the world.
    volume: Arc<VoxelVolume>,
    blocks: BlockMap,
    shared: Arc<BlockMap>,
    builder: BlockNavBuilder,
    /// The chunks each column made holds, and when it was last wanted.
    columns: HashMap<[i32; 2], (Vec<[i32; 3]>, Instant)>,
    making: HashSet<[i32; 2]>,
    /// Columns to make again, for a change, as soon as they can be.
    stale: HashSet<[i32; 2]>,
    /// Columns a way asked for wants made.
    asked: HashSet<[i32; 2]>,
    sent: mpsc::Sender<Made>,
    taken: mpsc::Receiver<Made>,
}

impl Ground {
    fn new(
        world: &World,
        entity: Entity,
        solids: Arc<dyn VoxelSolids>,
        volume: Arc<VoxelVolume>,
        corner: Vec3,
    ) -> Self {
        let voxel = solids.voxel();
        let cell = ((CELL / voxel).round() as i32).max(1);
        let size = cell as f32 * voxel;
        let walkers = walkers(world, size);
        let (sent, taken) = mpsc::channel();
        Self {
            solids,
            corner,
            dimension: dimension_of(world, entity),
            cell,
            size,
            volume,
            blocks: BlockMap::default(),
            shared: Arc::new(BlockMap::default()),
            builder: BlockNavBuilder::new(&walkers),
            columns: HashMap::new(),
            making: HashSet::new(),
            stale: HashSet::new(),
            asked: HashSet::new(),
            sent,
            taken,
        }
    }

    /// How many metres a column of chunks is across.
    fn side(&self) -> f32 {
        CHUNK as f32 * self.size
    }

    /// The column of chunks `at` in the world is over.
    fn column_of(&self, at: Vec3) -> [i32; 2] {
        let local = (at - self.corner) / self.side();
        [local.x.floor() as i32, local.z.floor() as i32]
    }

    /// The columns within `reach` metres of `at`, across.
    fn columns_round(&self, at: Vec3, reach: f32) -> Vec<[i32; 2]> {
        let (low, high) = (self.column_of(at - Vec3::splat(reach)), self.column_of(at + Vec3::splat(reach)));
        (low[1]..=high[1]).flat_map(|z| (low[0]..=high[0]).map(move |x| [x, z])).collect()
    }

    /// The middle of column `column` in the world, at height `y`.
    fn middle_of(&self, column: [i32; 2], y: f32) -> Vec3 {
        let side = self.side();
        let x = self.corner.x + (column[0] as f32 + 0.5) * side;
        let z = self.corner.z + (column[1] as f32 + 0.5) * side;
        Vec3::new(x, y, z)
    }

    /// The walker something `height` metres tall and `radius` round is.
    fn walker(&self, height: f32, radius: f32) -> Walker {
        walker_of(height, radius, self.size)
    }

    fn local(&self, at: Vec3, walker: Walker) -> [f32; 3] {
        let [x, y, z] = ((at - self.corner) / self.size).to_array();
        [x - walker.middle(), y, z - walker.middle()]
    }

    /// Starts making the columns wanted most that are not being made,
    /// while fewer than [`WORKERS`] are.
    fn make(&mut self, wanted: &[[i32; 2]]) {
        for column in wanted {
            if self.making.len() >= WORKERS {
                return;
            }
            if self.making.contains(column) || self.columns.contains_key(column) && !self.stale.contains(column) {
                continue;
            }
            self.stale.remove(column);
            self.making.insert(*column);
            let (solids, volume, sent) = (self.solids.clone(), self.volume.clone(), self.sent.clone());
            let (cell, corner, column) = (self.cell, self.corner, *column);
            std::thread::spawn(move || {
                let chunks = cells_of(&*solids, &volume, corner, cell, column);
                let _ = sent.send(Made { column, chunks });
            });
        }
    }

    /// Takes the columns made, in place of what they were. True when any
    /// was.
    fn take(&mut self, now: Instant) -> bool {
        let mut changed = false;
        while let Ok(made) = self.taken.try_recv() {
            self.making.remove(&made.column);
            let old = self.columns.remove(&made.column).map(|(keys, _)| keys).unwrap_or_default();
            for key in old {
                self.blocks.put_chunk(key, Chunk::filled(AIR));
            }
            let keys = made.chunks.iter().map(|(key, _)| *key).collect();
            for (key, chunk) in made.chunks {
                self.blocks.put_chunk(key, chunk);
            }
            self.columns.insert(made.column, (keys, now));
            changed = true;
        }
        changed
    }

    /// Lets go of the columns far from every agent and not wanted for a
    /// while. True when any was.
    fn let_go(&mut self, near: &[Vec3], now: Instant) -> bool {
        let side = self.side();
        let far = |column: [i32; 2]| {
            let middle = self.middle_of(column, 0.0);
            near.iter().all(|at| (Vec3::new(at.x, 0.0, at.z) - middle).abs().max_element() > LET_GO + side)
        };
        let gone: Vec<[i32; 2]> = self
            .columns
            .iter()
            .filter(|(column, (_, wanted))| now.duration_since(*wanted) > UNUSED && far(**column))
            .map(|(column, _)| *column)
            .collect();
        for column in &gone {
            for key in self.columns.remove(column).map(|(keys, _)| keys).unwrap_or_default() {
                self.blocks.put_chunk(key, Chunk::filled(AIR));
            }
            self.asked.remove(column);
        }
        !gone.is_empty()
    }

    /// Marks the columns the voxel boxes `changed` reach to be made again.
    fn changed(&mut self, changed: &[([i32; 3], [i32; 3])]) {
        let side = CHUNK as i32 * self.cell;
        for (low, high) in changed {
            let (least, most) = ([low[0] - 1, low[2] - 1], [high[0] + 1, high[2] + 1]);
            for z in least[1].div_euclid(side)..=most[1].div_euclid(side) {
                for x in least[0].div_euclid(side)..=most[0].div_euclid(side) {
                    if self.columns.contains_key(&[x, z]) || self.making.contains(&[x, z]) {
                        self.stale.insert([x, z]);
                    }
                }
            }
        }
    }
}

/// The walker something `height` metres tall is, on cells `size` metres a
/// side: stepping up and dropping as people do. It is a cell wide, however
/// wide it is: a walker of more stands only where every column under it is
/// within half a cell of the others, which ground of small voxels sloping
/// at all seldom is. What is solid is made [`CLEAR`] wider instead.
fn walker_of(height: f32, _radius: f32, size: f32) -> Walker {
    Walker::of(height, STEP, DROP, size)
}

/// The walkers a volume's cells `size` metres a side gather their places
/// into regions for: each agent type's, and those of people and of small
/// and large mobs.
fn walkers(world: &World, size: f32) -> Vec<Walker> {
    let mut walkers: Vec<Walker> = (0..crate::navigation::AGENT_TYPES)
        .map(|slot| crate::navigation::agent_type(world, slot as u8))
        .map(|kind| walker_of(kind.height, kind.radius, size).class())
        .collect();
    walkers
        .extend([(1.8, 0.35), (1.0, 0.3), (2.5, 0.5)].map(|(height, radius)| walker_of(height, radius, size).class()));
    let mut kept: Vec<Walker> = Vec::new();
    for walker in walkers {
        if !kept.contains(&walker) {
            kept.push(walker);
        }
    }
    kept
}

/// The cells of column `column` of chunks: from a chunk under its lowest
/// ground up to the first chunk over its highest with nothing solid in
/// it, each cell `cell` voxels a side and solid where any of its voxels
/// is; water where the sea or a river stands over an open cell.
fn cells_of(
    solids: &dyn VoxelSolids,
    volume: &VoxelVolume,
    corner: Vec3,
    cell: i32,
    column: [i32; 2],
) -> Vec<([i32; 3], Chunk)> {
    let side = CHUNK as i32 * cell;
    let (x0, z0) = (column[0] * side, column[1] * side);
    let voxel = solids.voxel();
    let tall = (volume.size.y / voxel).round() as i32;
    let heights = match solids.as_any().downcast_ref::<crate::voxel_volumes::Solids>() {
        Some(found) => found.0.heights([x0, z0], [side, side]),
        None => vec![0; (side * side) as usize],
    };
    let (Some(&lowest), Some(&highest)) = (heights.iter().min(), heights.iter().max()) else { return Vec::new() };
    // Where water stands over each column of cells, in cells from the volume's floor.
    let cells = CHUNK as i32;
    let size = cell as f32 * voxel;
    let sea = volume.sea_height(corner).map(|y| (y - corner.y) / size);
    // Rivers' water is read a metre apart, each cell taking that of the metre it is in: it is slow to read.
    let every = ((1.0 / size).round() as i32).max(1);
    let samples = (cells + every - 1) / every;
    let rivers: Vec<f32> = (0..samples * samples)
        .map(|at| {
            let (i, k) = ((at % samples) * every, (at / samples) * every);
            let h = heights[((k * cell + cell / 2) * side + i * cell + cell / 2) as usize];
            let x = corner.x + (x0 + i * cell) as f32 * voxel + size * 0.5 * every as f32;
            let z = corner.z + (z0 + k * cell) as f32 * voxel + size * 0.5 * every as f32;
            let ground = corner.y + (h + 1) as f32 * voxel;
            let river = voxear_render::voxel_water_at(volume, corner, Vec3::new(x, ground, z));
            river.map_or(f32::MIN, |w| (w.height - corner.y) / size)
        })
        .collect();
    let water: Vec<f32> = (0..cells * cells)
        .map(|at| {
            let (i, k) = (at % cells, at / cells);
            let river = rivers[((k / every) * samples + i / every) as usize];
            sea.map_or(river, |sea| sea.max(river))
        })
        .collect();
    // The voxels filled reach CLEAR past the column's sides, along X and Z.
    let margin = (CLEAR / voxel).round() as i32;
    let wide = side + 2 * margin;
    let mut solid = vec![false; (wide * side * wide) as usize];
    let mut chunks = Vec::new();
    let first = lowest.div_euclid(side) - 1;
    let mut ky = first.max(-1);
    while ky <= first + MOST_UP && ky * side < tall {
        solids.fill([x0 - margin, ky * side, z0 - margin], [wide, side, wide], &mut solid);
        let mut chunk = Chunk::filled(AIR);
        let mut any = false;
        let across = cell + 2 * margin;
        for k in 0..cells {
            for j in 0..cells {
                for i in 0..cells {
                    let filled = (0..across).any(|dz| {
                        (0..cell).any(|dy| {
                            let row = ((k * cell + dz) * side + j * cell + dy) * wide + i * cell;
                            solid[row as usize..(row + across) as usize].iter().any(|v| *v)
                        })
                    });
                    let y = (ky * cells + j) as f32;
                    let block = match filled {
                        true => GROUND,
                        false if y < water[(k * cells + i) as usize] => WATER,
                        false => AIR,
                    };
                    if block != AIR {
                        chunk.set(i as u32, j as u32, k as u32, block);
                        any |= block == GROUND;
                    }
                }
            }
        }
        if !chunk.is_air() {
            chunks.push(([column[0], ky, column[1]], chunk.tidied()));
        }
        // Past the highest ground, up to the first chunk with nothing solid in it.
        if (ky + 1) * side > highest && !any {
            break;
        }
        ky += 1;
    }
    chunks
}

/// Every endless volume's navigation, by its Obj.
#[derive(Default)]
struct Grounds {
    grounds: HashMap<Entity, Ground>,
}

/// The finder of ways over voxel ground that [`update`] keeps.
struct Finder(Arc<Mutex<Grounds>>);

/// The world's voxel navigation, kept by [`update`].
#[derive(Clone)]
pub struct VoxelNavigation(Arc<Mutex<Grounds>>);

impl VoxelWays for Finder {
    fn way(
        &self,
        _: &World,
        dimension: DimensionId,
        from: Vec3,
        to: Vec3,
        height: f32,
        radius: f32,
    ) -> Option<VoxelWay> {
        let straight = Some(VoxelWay { corners: vec![from, to], complete: true });
        let (nav, ask, ground) = {
            let mut grounds = self.0.lock().ok()?;
            let ground = grounds.grounds.values_mut().find(|ground| ground.dimension == dimension)?;
            // The cells along the way, made from now on.
            let flat = Vec3::new(to.x - from.x, 0.0, to.z - from.z);
            let steps = (flat.length().min(ALONG) / (ground.side() * 0.5)).ceil() as usize;
            let direction = flat.normalize_or_zero();
            for step in 0..=steps {
                let at = from + direction * (step as f32 * ground.side() * 0.5);
                let column = ground.column_of(at);
                ground.asked.insert(column);
            }
            ground.asked.extend(ground.columns_round(to, ground.side() * 0.5));
            let walker = ground.walker(height, radius);
            let mut ask = PathAsk::new(
                ground.local(from + Vec3::Y * 0.1 * ground.size, walker),
                ground.local(to, walker),
                walker,
            );
            (ask.most, ask.refine) = (LOOKED, Some(REFINED));
            (ask.across, ask.reach) = (2.0 / ground.size, 3.0 / ground.size);
            (ground.builder.nav().clone(), ask, (ground.corner, ground.size, walker))
        };
        let found = find_path(&nav, &ask);
        let (corner, size, walker) = ground;
        let world = |at: [f32; 3]| {
            let middle = walker.middle();
            corner + Vec3::new(at[0] + middle, at[1], at[2] + middle) * size
        };
        match found.status {
            // Where it stands has no cells yet: straight there until it has.
            PathStatus::None => straight,
            status => {
                let mut corners: Vec<Vec3> = found.corners.iter().map(|at| world(*at)).collect();
                if let Some(first) = corners.first_mut() {
                    *first = from;
                }
                if corners.len() < 2 {
                    corners.push(to);
                }
                Some(VoxelWay { corners, complete: status == PathStatus::Complete })
            }
        }
    }
}

impl VoxelNavigation {
    /// The navigation of volume `entity`'s ground as last made, its
    /// corner and how many metres a cell is.
    pub fn nav(&self, entity: Entity) -> Option<(Arc<BlockNav>, Vec3, f32)> {
        let grounds = self.0.lock().ok()?;
        grounds.grounds.get(&entity).map(|ground| (ground.builder.nav().clone(), ground.corner, ground.size))
    }

    /// Whether every column wanted is made and in the navigation.
    pub fn is_settled(&self) -> bool {
        self.0.lock().is_ok_and(|grounds| {
            grounds
                .grounds
                .values()
                .all(|ground| ground.making.is_empty() && ground.stale.is_empty() && ground.builder.is_settled())
        })
    }

    /// How many columns of chunks are made, over every volume.
    pub fn columns(&self) -> usize {
        self.0.lock().map_or(0, |grounds| grounds.grounds.values().map(|ground| ground.columns.len()).sum())
    }
}

/// The world's voxel navigation, begun where it was not: what finds ways
/// over voxel ground from now on, unless something else was set first.
pub fn ensure(world: &mut World) -> VoxelNavigation {
    if let Some(kept) = world.resource::<VoxelNavigation>() {
        return kept.clone();
    }
    let kept = VoxelNavigation(Arc::default());
    world.insert_resource(kept.clone());
    if !crate::voxel_ways::is_set(world) {
        crate::voxel_ways::set(world, Arc::new(Finder(kept.0.clone())));
    }
    kept
}

/// Where something that walks stands, in the world.
fn place_of(world: &World, entity: Entity) -> Option<Vec3> {
    let placed = world.get::<WorldTransform>(entity);
    world.get::<Transform>(entity).map(|local| voxear_terrain::corner(local, placed))
}

/// Keeps each endless volume's navigation up with its voxels and with
/// what walks on it: cells made round each Nav Agent and Block Mob and
/// along the ways asked for, made again where the volume changed, and let
/// go where nothing is near. Sets itself as what finds ways over voxel
/// ground, unless something else was set first. Cheap where nothing walks.
pub fn update(world: &mut World) {
    let walking: Vec<(Vec3, DimensionId)> = {
        let agents = world.query_ref::<(Entity, &crate::nav_agent::NavAgent)>().into_iter().map(|(entity, _)| entity);
        let mobs = world.query_ref::<(Entity, &crate::block_mob::BlockMob)>().into_iter().map(|(entity, _)| entity);
        let entities: HashSet<Entity> = agents.chain(mobs).collect();
        entities
            .into_iter()
            .filter_map(|entity| Some((place_of(world, entity)?, dimension_of(world, entity))))
            .collect()
    };
    let navigation = match world.resource::<VoxelNavigation>() {
        Some(kept) => kept.clone(),
        None if walking.is_empty() => return,
        None => ensure(world),
    };
    voxear_core::profile_scope!("Voxel navigation");
    let volumes: Vec<(Entity, Arc<dyn VoxelSolids>, Vec3)> = world
        .query_ref::<(Entity, &crate::physics::VoxelCollider, &Transform, Option<&WorldTransform>)>()
        .into_iter()
        .filter(|(_, collider, ..)| collider.0.as_any().is::<crate::voxel_volumes::Solids>())
        .map(|(entity, collider, local, placed)| (entity, collider.0.clone(), voxear_terrain::corner(local, placed)))
        .collect();
    let Ok(mut grounds) = navigation.0.lock() else { return };
    grounds.grounds.retain(|entity, _| volumes.iter().any(|(kept, ..)| kept == entity));
    let now = Instant::now();
    for (entity, solids, corner) in volumes {
        let Some(volume) =
            solids.as_any().downcast_ref::<crate::voxel_volumes::Solids>().map(|found| found.0.volume().clone())
        else {
            continue;
        };
        let renewed = match grounds.grounds.get_mut(&entity) {
            Some(ground) if Arc::ptr_eq(&ground.solids, &solids) => false,
            // Moved, or changed all over: made again from nothing.
            Some(ground) if ground.corner == corner => match solids.changed_from(&*ground.solids) {
                Some(changed) => {
                    ground.changed(&changed);
                    (ground.solids, ground.volume) = (solids.clone(), Arc::new(volume.clone()));
                    false
                }
                None => true,
            },
            _ => true,
        };
        if renewed {
            grounds.grounds.insert(entity, Ground::new(world, entity, solids, Arc::new(volume), corner));
        }
        let Some(ground) = grounds.grounds.get_mut(&entity) else { continue };
        let near: Vec<Vec3> =
            walking.iter().filter(|(_, dimension)| *dimension == ground.dimension).map(|(at, _)| *at).collect();
        let mut changed = ground.take(now);
        // What is wanted: round each agent, then along the ways asked for; nearest first.
        let mut wanted: Vec<[i32; 2]> = near.iter().flat_map(|at| ground.columns_round(*at, AROUND)).collect();
        wanted.extend(ground.asked.iter().copied());
        let distance = |column: &[i32; 2]| {
            let middle = ground.middle_of(*column, 0.0);
            near.iter().map(|at| (Vec3::new(at.x, 0.0, at.z) - middle).length()).fold(f32::MAX, f32::min)
        };
        wanted.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        wanted.dedup();
        for column in &wanted {
            if let Some((_, used)) = ground.columns.get_mut(column) {
                *used = now;
            }
        }
        ground.asked.retain(|column| !ground.columns.contains_key(column) || ground.stale.contains(column));
        ground.make(&wanted);
        changed |= ground.let_go(&near, now);
        if changed {
            ground.shared = Arc::new(ground.blocks.clone());
        }
        let faces = FACES.with(|faces| faces.clone());
        ground.builder.update(&ground.shared, &faces);
    }
}

thread_local! {
    /// What a cell is, as the navigation of blocks reads it: one set of faces
    /// for every volume, so the builder sees the same faces each frame.
    static FACES: Arc<[BlockFaces]> = faces();
}

/// Waits until every column wanted round what walks is made and in the
/// navigation, as long as `most`: for tests and tools that need the ways
/// now.
pub fn settle(world: &mut World, most: Duration) -> bool {
    let started = Instant::now();
    loop {
        update(world);
        let settled = world.resource::<VoxelNavigation>().is_some_and(VoxelNavigation::is_settled);
        if settled || started.elapsed() > most {
            return settled;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;
    use voxear_render::{VoxelEdit, VoxelGround, VoxelMaterial};

    use super::*;

    /// A flat volume 3 m deep, its corner at `corner`, with a wall of rock
    /// 2.5 m tall and half a metre thick along Z at X 8, from Z -1 to 9.
    fn walled(corner: Vec3) -> (World, Entity) {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = VoxelGround { height: 3.0, hills: 0.0, ..VoxelGround::default() };
        let wall = VoxelEdit::boxed(
            Vec3::new(8.0, 2.9, 4.0),
            Vec3::new(0.5, 2.6, 10.0),
            Quat::IDENTITY,
            Some(VoxelMaterial::Rock),
        );
        let volume = VoxelVolume {
            endless: true,
            size: Vec3::new(16.0, 16.0, 16.0),
            ground: Some(ground),
            edits: Arc::new(vec![wall]),
            ..VoxelVolume::default()
        };
        let obj = world.spawn((Transform::from_position(corner),));
        world.insert(obj, volume);
        (world, obj)
    }

    fn top(world: &World, obj: Entity, corner: Vec3, x: f32, z: f32) -> Vec3 {
        let volume = world.get::<VoxelVolume>(obj).unwrap();
        let from = corner + Vec3::new(x, 15.0, z);
        voxear_render::voxel_raycast(volume, corner, from, -Vec3::Y, 20.0).expect("the ground").point
    }

    /// The way from `from` to `to`, once the cells round an agent at `from` are made.
    fn way(world: &mut World, from: Vec3, to: Vec3) -> VoxelWay {
        crate::voxel_volumes::sync_colliders(world);
        assert!(settle(world, Duration::from_secs(120)), "the cells are made");
        let dimension = voxear_scene::DimensionId::LEVEL;
        crate::voxel_ways::finder(world).way(world, dimension, from, to, 1.8, 0.35).expect("a way")
    }

    /// Whether the way's corners cross the wall at X 8 between Z `least` and `most`, in the volume.
    fn crosses(way: &VoxelWay, corner: Vec3, least: f32, most: f32) -> bool {
        way.corners.windows(2).any(|pair| {
            let (a, b) = (pair[0] - corner, pair[1] - corner);
            if (a.x - 8.0).signum() == (b.x - 8.0).signum() {
                return false;
            }
            let z = a.z + (b.z - a.z) * (8.0 - a.x) / (b.x - a.x);
            z > least - 0.2 && z < most + 0.2
        })
    }

    #[test]
    fn a_way_goes_round_a_wall_and_round_the_other_end_once_one_end_is_shut() {
        let corner = Vec3::new(-20.0, -5.0, 30.0);
        let (mut world, obj) = walled(corner);
        let (from, to) = (top(&world, obj, corner, 4.0, 5.0), top(&world, obj, corner, 12.0, 5.0));
        let walker = world.spawn((Transform::from_position(from),));
        world.insert(walker, crate::nav_agent::NavAgent::default());
        let found = way(&mut world, from, to);
        assert!(found.complete, "{found:?}");
        assert!(!crosses(&found, corner, -1.0, 9.0), "never through the wall: {:?}", found.corners);
        let north = found.corners.iter().any(|at| at.z - corner.z > 9.0);
        assert!(north, "round its nearer end, to the north: {:?}", found.corners);
        let end = *found.corners.last().unwrap();
        assert!((end - to).length() < 0.5, "to where it was sent: {end} for {to}");
        // That end walled off too: round the other.
        let shut = VoxelEdit::boxed(
            Vec3::new(8.0, 2.9, 14.0),
            Vec3::new(0.5, 2.6, 10.0),
            Quat::IDENTITY,
            Some(VoxelMaterial::Rock),
        );
        let mut volume = world.get::<VoxelVolume>(obj).unwrap().clone();
        volume.edits = Arc::new(volume.edits.iter().cloned().chain([shut]).collect());
        world.insert(obj, volume);
        let found = way(&mut world, from, to);
        assert!(found.complete, "{found:?}");
        assert!(!crosses(&found, corner, -1.0, 19.0), "never through either wall: {:?}", found.corners);
        assert!(found.corners.iter().any(|at| at.z - corner.z < -1.0), "round the south end: {:?}", found.corners);
    }

    #[test]
    fn a_way_goes_round_a_pit_too_deep_to_climb_out_of() {
        let corner = Vec3::new(0.0, 0.0, 0.0);
        let (mut world, obj) = walled(corner);
        let pit = VoxelEdit::boxed(Vec3::new(4.0, 0.5, -6.0), Vec3::new(3.0, 3.0, 3.0), Quat::IDENTITY, None);
        let mut volume = world.get::<VoxelVolume>(obj).unwrap().clone();
        volume.edits = Arc::new(volume.edits.iter().cloned().chain([pit]).collect());
        world.insert(obj, volume);
        let (from, to) = (top(&world, obj, corner, 4.0, -10.0), top(&world, obj, corner, 4.0, -2.0));
        let walker = world.spawn((Transform::from_position(from),));
        world.insert(walker, crate::nav_agent::NavAgent::default());
        let found = way(&mut world, from, to);
        assert!(found.complete, "{found:?}");
        // The straight line crosses the pit, 2.5 m deep and 3 m across: the way goes round it, along its rim, which
        // is solid a quarter of a metre further in as the cells are.
        let off = found.corners.iter().map(|at| (at.x - corner.x - 4.0).abs()).fold(0.0, f32::max);
        assert!(off > 1.2, "round the pit: {:?}", found.corners);
    }

    /// What a column of cells costs to make, and a way across a few of them
    /// to find: run by hand, `--ignored --nocapture`.
    #[test]
    #[ignore = "a measure, not a check"]
    fn measure_making_cells_and_finding_ways() {
        use voxear_render::VoxelGenerator;
        let starter = |name: &str| voxear_terrain::generate::starters().into_iter().find(|s| s.name == name).unwrap();
        let hills = Arc::new(starter("Rolling Hills").world(7, 1.0));
        let floor = hills.height_at(8.0, 8.0) - 24.0;
        let generated = |world: Arc<voxear_terrain::generate::WorldGen>, floor: f32| VoxelVolume {
            endless: true,
            size: Vec3::new(16.0, 64.0, 16.0),
            generator: Some(VoxelGenerator { world, floor, at: [0.0, 0.0], biomes: None }),
            ..VoxelVolume::default()
        };
        let own = VoxelVolume {
            endless: true,
            size: Vec3::new(16.0, 16.0, 16.0),
            ground: Some(VoxelGround { height: 3.0, hills: 2.0, ..VoxelGround::default() }),
            ..VoxelVolume::default()
        };
        for (name, volume) in [("own hills", own), ("Rolling Hills", generated(hills, floor))] {
            let mut world = World::new();
            voxear_scene::register_obj_components(&mut world);
            let obj = world.spawn((Transform::default(),));
            world.insert(obj, volume);
            crate::voxel_volumes::sync_colliders(&mut world);
            let solids = world.get::<crate::physics::VoxelCollider>(obj).unwrap().0.clone();
            let volume = world.get::<VoxelVolume>(obj).unwrap().clone();
            let mut took = Vec::new();
            for column in [[0, 0], [1, 0], [0, 1], [1, 1]] {
                let started = Instant::now();
                let chunks = cells_of(&*solids, &volume, Vec3::ZERO, 4, column);
                took.push((started.elapsed(), chunks.len()));
            }
            let high = |x: f32, z: f32| {
                let from = Vec3::new(x, volume.size.y - 1.0, z);
                voxear_render::voxel_raycast(&volume, Vec3::ZERO, from, -Vec3::Y, volume.size.y)
                    .expect("the ground")
                    .point
            };
            let from = high(4.0, 4.0);
            let walker = world.spawn((Transform::from_position(from),));
            world.insert(walker, crate::nav_agent::NavAgent::default());
            let started = Instant::now();
            assert!(settle(&mut world, Duration::from_secs(300)));
            let settled = started.elapsed();
            let columns = world.resource::<VoxelNavigation>().unwrap().columns();
            let to = high(20.0, 22.0);
            let finder = crate::voxel_ways::finder(&world);
            let started = Instant::now();
            let way = finder.way(&world, voxear_scene::DimensionId::LEVEL, from, to, 1.8, 0.35);
            let found = started.elapsed();
            println!(
                "{name}: columns {took:?}; {columns} round an agent in {settled:?}; a way of 25 m in {found:?}: {way:?}"
            );
        }
    }
}
