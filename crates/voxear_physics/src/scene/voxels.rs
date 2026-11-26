//! Colliding with small voxels: an Obj's [`VoxelCollider`] says which of
//! its voxels are solid, and the scene gives it static meshes of their
//! faces in chunks of [`CHUNK`] voxels a side, made only round what moves:
//! the characters and the bodies with a Rigidbody. Ground without end is
//! never made whole; each chunk is made once something comes near, in the
//! background as it comes within a few metres, or at once for one within
//! a metre of it, and let go once all have gone well away.
//!
//! The voxels are the Obj's own to say ([`VoxelSolids`]): an endless Voxel
//! Volume's ground and its trees' logs, worked out on the CPU as the GPU
//! draws them.

use std::sync::{Arc, Mutex};

use super::*;

/// Which voxels of an Obj are solid, from its place along +X, +Y and +Z,
/// each [`voxel`](VoxelSolids::voxel) metres a side.
pub trait VoxelSolids: Send + Sync + 'static {
    /// How large a voxel is, in metres.
    fn voxel(&self) -> f32;

    /// Whether each voxel of the box from `low`, `size` voxels a side, is
    /// solid: written to `solid`, along X first, then Y, then Z.
    fn fill(&self, low: [i32; 3], size: [i32; 3], solid: &mut [bool]);

    /// The boxes of voxels, least and most corners, where these may be
    /// solid where `older` is not, or not where it is: none where any may.
    fn changed_from(&self, older: &dyn VoxelSolids) -> Option<Vec<([i32; 3], [i32; 3])>>;

    fn as_any(&self) -> &dyn std::any::Any;
}

/// An Obj's solid voxels, which characters and bodies collide with. Given
/// again with new voxels, the chunks where they changed are made again.
#[derive(Clone)]
pub struct VoxelCollider(pub Arc<dyn VoxelSolids>);

impl Component for VoxelCollider {}

impl std::fmt::Debug for VoxelCollider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("VoxelCollider").field(&self.0.voxel()).finish()
    }
}

/// How many voxels a chunk's shape is a side.
pub const CHUNK: i32 = 32;
/// How near something moving must be to a chunk, in metres, for it to be
/// made at once; for it to be made in the background; and how far all must
/// be for it to be let go, two chunks and this further.
const AT_ONCE: f32 = 1.0;
const AHEAD: f32 = 5.0;
const LET_GO: f32 = 4.0;
/// How far ahead, in seconds, where something moving will be is looked at
/// as well as where it is.
const LOOK_AHEAD: f32 = 0.5;
/// How many workers make an Obj's chunks in the background at most.
const WORKERS: usize = 6;

/// The chunks waiting to be made in the background, the nearest last, each
/// with its asking's number; the voxels they are made of; and how many
/// workers are taking them.
struct Queue {
    waiting: Vec<([i32; 3], u64, f32)>,
    /// Those a worker is making, by their asking's number.
    taken: HashSet<u64>,
    solids: Arc<dyn VoxelSolids>,
    workers: usize,
}

/// A chunk made in the background: the chunk, which asking it answers, and
/// its shape where it has faces.
type MadeVoxels = ([i32; 3], u64, Option<crate::MadeShape>);

/// An Obj's voxels' bodies: the voxels they were made from, where the Obj
/// was, its layer, and each chunk made, with its body and shape where it
/// has faces.
pub(super) struct VoxelBodies {
    solids: Arc<dyn VoxelSolids>,
    corner: Vec3,
    layer: u16,
    voxel: f32,
    chunks: HashMap<[i32; 3], Option<(BodyId, ShapeId)>>,
    /// The chunks being made in the background, by the number of the asking
    /// a made chunk must answer to be kept; and those made since.
    making: HashMap<[i32; 3], u64>,
    asked: u64,
    made: Arc<Mutex<Vec<MadeVoxels>>>,
    queue: Arc<Mutex<Queue>>,
    /// Chunks whose voxels changed: their old bodies stay until they are made again.
    stale: HashSet<[i32; 3]>,
}

impl VoxelBodies {
    pub(super) fn shift(&mut self, by: Vec3) {
        self.corner += by;
    }

    /// How many chunks have bodies.
    #[cfg(test)]
    pub(super) fn bodies(&self) -> usize {
        self.chunks.values().filter(|chunk| chunk.is_some()).count()
    }
}

/// The faces of chunk `key`'s solid voxels that face open ones, as a mesh in
/// metres from the chunk's least corner, those side by side in a plane
/// joined into rectangles; none where it has no faces.
pub(super) fn chunk_shape(solids: &dyn VoxelSolids, key: [i32; 3]) -> Option<crate::MadeShape> {
    let (points, indices) = chunk_faces(solids, key);
    if indices.is_empty() {
        return None;
    }
    crate::MadeShape::mesh(&points, &indices)
}

/// Chunk `key`'s faces, as [`chunk_shape`] makes them: points and triangles.
pub(super) fn chunk_faces(solids: &dyn VoxelSolids, key: [i32; 3]) -> (Vec<Vec3>, Vec<u32>) {
    const N: i32 = CHUNK;
    const S: i32 = N + 2;
    let mut solid = vec![false; (S * S * S) as usize];
    let low = key.map(|a| a * N - 1);
    solids.fill(low, [S; 3], &mut solid);
    let (mut points, mut indices) = (Vec::new(), Vec::new());
    if solid.iter().all(|&s| s) || solid.iter().all(|&s| !s) {
        return (points, indices);
    }
    let at = |c: [i32; 3]| solid[((c[0] + 1) + S * ((c[1] + 1) + S * (c[2] + 1))) as usize];
    let voxel = solids.voxel();
    let mut mask = vec![false; (N * N) as usize];
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for out in [-1i32, 1] {
            for slice in 0..N {
                // Which voxels of the slice have a face this way: solid, with open air beyond.
                for j in 0..N {
                    for i in 0..N {
                        let mut c = [0; 3];
                        (c[axis], c[u], c[v]) = (slice, i, j);
                        let mut beyond = c;
                        beyond[axis] += out;
                        mask[(i + N * j) as usize] = at(c) && !at(beyond);
                    }
                }
                // Rectangles of them, each as wide along u and then as long along v as it goes.
                let plane = (slice + i32::from(out > 0)) as f32 * voxel;
                for j in 0..N {
                    let mut i = 0;
                    while i < N {
                        if !mask[(i + N * j) as usize] {
                            i += 1;
                            continue;
                        }
                        let mut w = 1;
                        while i + w < N && mask[(i + w + N * j) as usize] {
                            w += 1;
                        }
                        let mut h = 1;
                        while j + h < N && (i..i + w).all(|k| mask[(k + N * (j + h)) as usize]) {
                            h += 1;
                        }
                        for jj in j..j + h {
                            for k in i..i + w {
                                mask[(k + N * jj) as usize] = false;
                            }
                        }
                        let corner = |a: i32, b: i32| {
                            let mut p = Vec3::ZERO;
                            p[axis] = plane;
                            p[u] = a as f32 * voxel;
                            p[v] = b as f32 * voxel;
                            p
                        };
                        let first = points.len() as u32;
                        points.extend([corner(i, j), corner(i + w, j), corner(i + w, j + h), corner(i, j + h)]);
                        // Wound to face out of the voxel: along +axis, u then v turns counter-clockwise.
                        let quad = if out > 0 { [0, 1, 2, 0, 2, 3] } else { [0, 2, 1, 0, 3, 2] };
                        indices.extend(quad.map(|q| first + q));
                        i += w;
                    }
                }
            }
        }
    }
    (points, indices)
}

impl PhysicsScene {
    /// Whether the chunks of `entity`'s voxels within the box from `low` to
    /// `high`, in the world, are made from its voxels as they are now: none
    /// there waiting to be made again after they changed. A body put where
    /// voxels were just taken away waits for this, so it does not start
    /// inside their old shapes.
    pub fn voxels_ready(&self, entity: Entity, low: Vec3, high: Vec3) -> bool {
        let Some(kept) = self.voxel_bodies.get(&entity) else { return true };
        let side = CHUNK as f32 * kept.voxel;
        let (least, most) = (((low - kept.corner) / side).floor().as_ivec3(), ((high - kept.corner) / side).floor().as_ivec3());
        let inside = |key: &[i32; 3]| (0..3).all(|a| key[a] >= least[a] && key[a] <= most[a]);
        !kept.stale.iter().any(inside) && !kept.making.keys().any(inside)
    }

    /// Gives each Obj with a [`VoxelCollider`] the chunks of its voxels'
    /// faces round what moves, made again where its voxels changed, and
    /// moved with it.
    pub(super) fn sync_voxels(&mut self, world: &World) {
        voxear_core::profile_scope!("Voxel Bodies");
        let found: Vec<(Entity, Arc<dyn VoxelSolids>, Vec3)> = world
            .query_ref::<(Entity, &VoxelCollider, &Transform, Option<&WorldTransform>)>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, collider, local, placed)| {
                (entity, collider.0.clone(), voxear_terrain::corner(local, placed))
            })
            .collect();
        let gone: Vec<Entity> =
            self.voxel_bodies.keys().copied().filter(|entity| !found.iter().any(|(e, ..)| e == entity)).collect();
        for entity in gone {
            self.remove_voxels(entity);
        }
        if found.is_empty() {
            return;
        }
        // What moves: the characters, and the bodies physics or the game moves; each where it is and where it will be.
        // Each with how far it reaches from its place: a long body, a felled tree, wants chunks as far as its ends.
        let walkers = self
            .characters
            .values()
            .map(|walker| (walker.placed, walker.state.map_or(Vec3::ZERO, |s| s.velocity), 0.0));
        let bodies = self.bodies.values().filter(|kept| kept.key.rigidbody.is_some());
        let bodies = bodies.map(|kept| (kept.placed.0, self.world.velocity(kept.body).0, kept.reach));
        let movers: Vec<(Vec3, bool, f32)> = walkers
            .chain(bodies)
            .flat_map(|(at, going, far)| [(at, true, far), (at + going * LOOK_AHEAD, false, far)])
            .collect();
        for (entity, solids, corner) in found {
            let layer = layer(world, entity);
            let voxel = solids.voxel().max(1e-4);
            let fits = self.voxel_bodies.get(&entity).is_some_and(|kept| kept.layer == layer && kept.voxel == voxel);
            if !fits {
                self.remove_voxels(entity);
                let kept = VoxelBodies {
                    solids: solids.clone(),
                    corner,
                    layer,
                    voxel,
                    chunks: HashMap::new(),
                    making: HashMap::new(),
                    asked: 0,
                    made: Arc::default(),
                    queue: Arc::new(Mutex::new(Queue {
                        waiting: Vec::new(),
                        taken: HashSet::new(),
                        solids: solids.clone(),
                        workers: 0,
                    })),
                    stale: HashSet::new(),
                };
                self.voxel_bodies.insert(entity, kept);
            }
            let mut kept = self.voxel_bodies.remove(&entity).expect("kept above");
            let side = CHUNK as f32 * voxel;
            let at = |key: &[i32; 3]| corner + Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
            // New voxels: the chunks where they changed are stale, and what is being made of them is not wanted.
            if !Arc::ptr_eq(&kept.solids, &solids) {
                let changed = solids.changed_from(&*kept.solids);
                let touched = |key: &[i32; 3]| match &changed {
                    None => true,
                    Some(boxes) => boxes.iter().any(|(least, most)| {
                        (0..3).all(|a| key[a] * CHUNK <= most[a] + 1 && (key[a] + 1) * CHUNK > least[a] - 1)
                    }),
                };
                let stale: Vec<[i32; 3]> =
                    kept.chunks.keys().chain(kept.making.keys()).filter(|k| touched(k)).copied().collect();
                for key in stale {
                    kept.making.remove(&key);
                    if kept.chunks.contains_key(&key) {
                        kept.stale.insert(key);
                    }
                }
                kept.solids = solids.clone();
            }
            if kept.corner != corner {
                for (key, chunk) in &kept.chunks {
                    if let Some((body, _)) = chunk {
                        self.world.set_transform(*body, at(key), Quat::IDENTITY);
                    }
                }
                kept.corner = corner;
            }
            // Which chunks are wanted, and which now, round each thing that moves: as far as [`AHEAD`] across,
            // and up and down no further than a chunk, as things go mostly across.
            let mut now: HashSet<[i32; 3]> = HashSet::new();
            let mut wanted: HashMap<[i32; 3], f32> = HashMap::new();
            for (place, there, far) in &movers {
                let reach = Vec3::new(AHEAD.max(side) + far, side + far, AHEAD.max(side) + far);
                let local = *place - corner;
                let low = ((local - reach) / side).floor().as_ivec3();
                let high = ((local + reach) / side).floor().as_ivec3();
                for z in low.z..=high.z {
                    for y in low.y..=high.y {
                        for x in low.x..=high.x {
                            let key = [x, y, z];
                            let (least, most) = (at(&key), at(&key) + Vec3::splat(side));
                            let off = place.clamp(least, most) - *place;
                            if *there && off.length() <= AT_ONCE {
                                now.insert(key);
                            }
                            if (off / reach).abs().max_element() <= 1.0 {
                                let nearest = wanted.entry(key).or_insert(f32::MAX);
                                *nearest = nearest.min(off.length());
                            }
                        }
                    }
                }
            }
            // Those made in the background since, kept where they answer the last asking for them.
            let made = std::mem::take(&mut *kept.made.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
            let mut ready = Vec::new();
            for (key, asking, shape) in made {
                if kept.making.get(&key) == Some(&asking) {
                    kept.making.remove(&key);
                    ready.push((key, shape));
                }
            }
            // Those wanted now made on every core; the rest of those wanted asked of the workers.
            let ready_keys: HashSet<[i32; 3]> = ready.iter().map(|(key, _)| *key).collect();
            let missing = |key: &[i32; 3]| {
                (!kept.chunks.contains_key(key) || kept.stale.contains(key)) && !ready_keys.contains(key)
            };
            let at_once: Vec<[i32; 3]> = now.iter().filter(|key| missing(key)).copied().collect();
            for key in &at_once {
                kept.making.remove(key);
            }
            let shapes = voxear_terrain::workers::each(&at_once, |key| chunk_shape(&*solids, *key));
            ready.extend(at_once.into_iter().zip(shapes));
            // Those to be made in the background: those asked before and still wanted, and those wanted now, the
            // nearest first; those no longer wanted dropped.
            kept.making.retain(|key, _| wanted.contains_key(key));
            for (key, _) in &wanted {
                if missing(key) && !now.contains(key) && !kept.making.contains_key(key) {
                    kept.asked += 1;
                    kept.making.insert(*key, kept.asked);
                }
            }
            {
                let mut queue = kept.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                queue.solids = solids.clone();
                let taken = std::mem::take(&mut queue.taken);
                queue.waiting = kept
                    .making
                    .iter()
                    .filter(|(_, asking)| !taken.contains(*asking))
                    .map(|(key, asking)| (*key, *asking, wanted[key]))
                    .collect();
                queue.taken = taken;
                queue.waiting.sort_by(|a, b| b.2.total_cmp(&a.2));
                let more = WORKERS.saturating_sub(queue.workers).min(queue.waiting.len());
                queue.workers += more;
                for _ in 0..more {
                    let (queue, made) = (kept.queue.clone(), kept.made.clone());
                    voxear_terrain::workers::spawn(move || {
                        loop {
                            let (key, asking, solids) = {
                                let mut queue = queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                                match queue.waiting.pop() {
                                    Some((key, asking, _)) => {
                                        queue.taken.insert(asking);
                                        (key, asking, queue.solids.clone())
                                    }
                                    None => {
                                        queue.workers -= 1;
                                        return;
                                    }
                                }
                            };
                            let shape = chunk_shape(&*solids, key);
                            made.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push((key, asking, shape));
                            queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).taken.remove(&asking);
                        }
                    });
                }
            }
            for (key, shape) in ready {
                if let Some(Some((body, shape))) = kept.chunks.remove(&key) {
                    self.world.remove_body(body);
                    self.world.release_shape(shape);
                    self.owners.remove(&body);
                }
                // Bodies asleep on voxels that changed wake, to fall into what was carved under them.
                if kept.stale.remove(&key) {
                    let (least, most) = (at(&key) - Vec3::splat(AT_ONCE), at(&key) + Vec3::splat(side + AT_ONCE));
                    let near = self.bodies.values().filter(|kept| kept.key.rigidbody.is_some_and(|rb| !rb.kinematic));
                    let near: Vec<BodyId> = near
                        .filter(|kept| kept.placed.0.clamp(least, most) == kept.placed.0)
                        .map(|kept| kept.body)
                        .collect();
                    for body in near {
                        self.world.set_active(body, true);
                    }
                }
                let Some(shape) = shape else {
                    kept.chunks.insert(key, None);
                    continue;
                };
                let shape = self.world.keep_shape(shape);
                let mut desc = BodyDesc::new(shape, Motion::Static).at(at(&key), Quat::IDENTITY);
                desc.friction = Collider::default().friction;
                desc.user_data = entity.to_bits();
                desc.layer = layer;
                let Some(body) = self.world.add_body(&desc) else {
                    self.world.release_shape(shape);
                    tracing::warn!("physics is full; a chunk of voxels is left out");
                    continue;
                };
                self.owners.insert(body, entity);
                kept.chunks.insert(key, Some((body, shape)));
            }
            // Chunks nothing has come near for a while let go.
            let keep = 2.0 * side + LET_GO;
            let far = |key: &[i32; 3]| {
                let (least, most) = (at(key), at(key) + Vec3::splat(side));
                movers.iter().all(|(place, _, reach)| place.clamp(least, most).distance(*place) > keep + reach)
            };
            let gone: Vec<[i32; 3]> = kept.chunks.keys().filter(|key| far(key)).copied().collect();
            for key in gone {
                if let Some(Some((body, shape))) = kept.chunks.remove(&key) {
                    self.world.remove_body(body);
                    self.world.release_shape(shape);
                    self.owners.remove(&body);
                }
                kept.stale.remove(&key);
            }
            self.voxel_bodies.insert(entity, kept);
        }
    }

    pub(super) fn remove_voxels(&mut self, entity: Entity) {
        let Some(kept) = self.voxel_bodies.remove(&entity) else { return };
        kept.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).waiting.clear();
        for (body, shape) in kept.chunks.into_values().flatten() {
            self.world.remove_body(body);
            self.world.release_shape(shape);
            self.owners.remove(&body);
        }
        self.forget(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat ground `top` voxels high, 1/16 m voxels, with a post one voxel
    /// thick standing on it at voxel column (`post`, `post`).
    struct Flat {
        top: i32,
        post: Option<i32>,
    }

    impl VoxelSolids for Flat {
        fn voxel(&self) -> f32 {
            1.0 / 16.0
        }

        fn fill(&self, low: [i32; 3], size: [i32; 3], solid: &mut [bool]) {
            for z in 0..size[2] {
                for y in 0..size[1] {
                    for x in 0..size[0] {
                        let c = [low[0] + x, low[1] + y, low[2] + z];
                        let post = self.post.is_some_and(|p| c[0] == p && c[2] == p && c[1] < self.top + 32);
                        solid[(x + size[0] * (y + size[1] * z)) as usize] = c[1] < self.top || post;
                    }
                }
            }
        }

        fn changed_from(&self, _older: &dyn VoxelSolids) -> Option<Vec<([i32; 3], [i32; 3])>> {
            None
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn a_chunks_faces_are_joined_into_rectangles_facing_out() {
        // Ground whose top is in the middle of the chunk: one rectangle, facing up.
        let flat = Flat { top: 10, post: None };
        let (points, indices) = chunk_faces(&flat, [0, 0, 0]);
        assert_eq!((points.len(), indices.len()), (4, 6), "one rectangle");
        assert!(points.iter().all(|p| (p.y - 10.0 / 16.0).abs() < 1e-6), "{points:?}");
        let [a, b, c] = [0, 1, 2].map(|i| points[indices[i] as usize]);
        assert!((b - a).cross(c - a).y > 0.0, "facing up");
        // A post up through the chunk: its four sides, beside the ground's top split round it.
        let post = Flat { top: 10, post: Some(5) };
        let (_, indices) = chunk_faces(&post, [0, 0, 0]);
        assert!((4 + 3..=4 + 8).contains(&(indices.len() / 6)), "{} rectangles", indices.len() / 6);
        // A chunk under the ground has none, as has one over it.
        assert!(chunk_faces(&flat, [0, -1, 0]).1.is_empty());
        assert!(chunk_faces(&flat, [0, 1, 0]).1.is_empty());
    }

    #[test]
    fn a_body_falls_onto_voxels_and_a_character_stands_on_them_and_bumps_into_a_post() {
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = world.spawn((Transform::from_position(Vec3::new(-8.0, 0.0, -8.0)),));
        let solids: Arc<dyn VoxelSolids> = Arc::new(Flat { top: 16, post: Some(8 * 16 + 24) });
        world.insert(ground, VoxelCollider(solids));
        let falling = world.spawn((Transform::from_position(Vec3::new(0.0, 4.0, 0.0)),));
        world.insert(falling, Collider::default());
        world.insert(falling, Rigidbody::default());
        let walker = world.spawn((Transform::from_position(Vec3::new(1.0, 1.2, 1.53)),));
        world.insert(walker, CharacterController::default());
        let mut physics = PhysicsScene::new();
        for _ in 0..180 {
            physics.step(&mut world, 1.0 / 60.0);
            physics.move_character(&mut world, walker, Vec3::new(0.0, -2.0, 0.0), 1.0 / 60.0);
        }
        // The ground's top is a metre up from the Obj's place: the crate of a metre rests on it.
        let y = world.get::<Transform>(falling).unwrap().position.y;
        assert!((y - 1.5).abs() < 0.05, "the crate rests at {y}");
        let state = physics.character_state(walker).expect("a character");
        assert_eq!(state.ground, crate::Ground::Ground, "{state:?}");
        assert!((state.position.y - 1.0).abs() < 0.1, "{state:?}");
        assert!(physics.voxel_bodies[&ground].bodies() > 0);
        // Walking along +X, it stops at the post, whose near face is at x = 1.5.
        for _ in 0..120 {
            physics.step(&mut world, 1.0 / 60.0);
            physics.move_character(&mut world, walker, Vec3::new(2.0, -2.0, 0.0), 1.0 / 60.0);
        }
        let x = physics.character_state(walker).unwrap().position.x;
        assert!(x < 1.5 && x > 1.0, "it stopped at the post, at {x}");
    }
}
