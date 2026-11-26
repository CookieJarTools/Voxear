//! Mobs on endless Voxel Volumes: each kind a volume lists is a prefab
//! spawned on its ground round the player while the game plays, as many to
//! a hectare as it says, on the ground it says and within or out of the
//! World Areas it says, and taken away again once the player is far from
//! it. A prefab with a Block Mob walks the volume's ground: straight to
//! where it is sent, and wandering about where it was spawned.
//!
//! ```toml
//! [[obj.voxel_volume.mobs]]
//! prefab = "…the prefab's number…"
//! per_hectare = 3.0
//! on = ["grass"]
//! near = 20.0     # spawned no nearer the player than this, in metres
//! far = 70.0      # nor further; taken away beyond it and a fifth again
//! most = 6
//! within = ["…a Pasture tag's number…"]
//! ```

use serde::{Deserialize, Serialize};
use voxear_assets::AssetGuid;
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{VoxelMaterial, VoxelVolume};
use voxear_scene::{Destroying, DimensionId, Transform, WorldTransform};

/// How often a volume's mobs are counted and spawned, in seconds.
const EVERY: f32 = 0.5;
/// How many places are tried for a mob each time, at most.
const TRIES: u32 = 4;
/// The steepest ground a mob is spawned on or wanders to, as the least
/// upward part of its ground's normal: about 45 degrees.
const LEVEL_ENOUGH: f32 = 0.7;

/// A kind of mob spawned on an endless volume's ground, as a scene writes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelMobsData {
    /// The prefab spawned: none for nothing yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefab: Option<AssetGuid>,
    /// How many to a hectare between `near` and `far` of the player, up to
    /// `most` at once.
    pub per_hectare: f32,
    pub most: u32,
    /// The ground it spawns on: grass where none.
    pub on: Vec<VoxelMaterial>,
    /// How near the player and how far from it it spawns, in metres.
    pub near: f32,
    pub far: f32,
    /// The World Areas it spawns in, by their tags, and those it never
    /// spawns in: anywhere where it names none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub within: Vec<AssetGuid>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub never_within: Vec<AssetGuid>,
}

impl Default for VoxelMobsData {
    fn default() -> Self {
        Self {
            prefab: None,
            per_hectare: 2.0,
            most: 6,
            on: vec![VoxelMaterial::Grass],
            near: 20.0,
            far: 70.0,
            within: Vec::new(),
            never_within: Vec::new(),
        }
    }
}

impl VoxelMobsData {
    /// How many it keeps round the player: its density over the ring it
    /// spawns in, at most `most`.
    pub fn wanted(&self) -> u32 {
        let (near, far) = (self.near.max(0.0), self.far.max(self.near));
        let hectares = std::f32::consts::PI * (far * far - near * near) / 10_000.0;
        ((self.per_hectare.max(0.0) * hectares).round() as u32).min(self.most)
    }
}

/// A mob spawned on a volume's ground: the volume, and which of its kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoxelMobSpawned {
    pub volume: Entity,
    pub kind: usize,
}

impl Component for VoxelMobSpawned {}

/// When each volume's mobs are next counted, and what spawns are drawn from.
#[derive(Clone, Debug, Default)]
pub struct VoxelMobClock {
    wait: f32,
    seed: u32,
    /// The prefabs that could not be spawned, not tried again.
    failed: Vec<AssetGuid>,
}

/// Tries prefab `prefab` no more: it could not be spawned.
pub fn failed(world: &mut World, prefab: AssetGuid) {
    let mut clock = world.resource::<VoxelMobClock>().cloned().unwrap_or_default();
    clock.failed.push(prefab);
    world.insert_resource(clock);
}

impl VoxelMobClock {
    fn chance(&mut self) -> f32 {
        self.seed = self.seed.max(1);
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Whether dimension `dimension` has an endless Voxel Volume's ground.
pub fn has_voxel_ground(world: &World, dimension: DimensionId) -> bool {
    world
        .query_ref::<(Entity, &VoxelVolume)>()
        .into_iter()
        .any(|(entity, volume)| volume.endless && voxear_scene::dimension_of(world, entity) == dimension)
}

fn corner_of(local: &Transform, placed: Option<&WorldTransform>) -> Vec3 {
    placed.map_or(local.position, |placed| placed.0.w_axis.truncate())
}

/// Where one stands on an endless volume's ground of dimension `dimension`
/// straight under `above`, and what the ground is there: none where the
/// ground there is too steep, or there is none under it.
pub fn standing(world: &World, dimension: DimensionId, above: Vec3) -> Option<(Vec3, VoxelMaterial)> {
    let volumes = world.query_ref::<(Entity, &VoxelVolume, &Transform, Option<&WorldTransform>)>();
    let (volume, corner) = volumes
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && voxear_scene::dimension_of(world, *entity) == dimension)
        .map(|(_, volume, local, placed)| (volume, corner_of(local, placed)))
        .find(|(volume, corner)| above.y >= corner.y && above.y - corner.y <= volume.size.y + 64.0)?;
    let top = above.y.min(corner.y + volume.size.y);
    let from = Vec3::new(above.x, top, above.z);
    let hit = voxear_render::voxel_raycast(volume, corner, from, Vec3::NEG_Y, top - corner.y + 1.0)?;
    if hit.normal.y < LEVEL_ENOUGH {
        return None;
    }
    let local = hit.point - corner;
    let (_, material) = voxear_render::voxel_ground_at(volume, local.x, local.z)?;
    Some((hit.point, material))
}

/// Where the player is, as mobs are spawned round it: the game's viewer.
fn player(world: &World) -> Option<Vec3> {
    crate::terrain::viewer(world)
}

/// What a volume's mobs want this time round: each kind's prefab, and
/// where to spawn one, or the mobs to take away.
pub struct MobChanges {
    pub spawn: Vec<(Entity, usize, AssetGuid, Vec3, f32)>,
    pub remove: Vec<Entity>,
}

/// Counts every endless volume's mobs round the player, `delta` seconds
/// on: those too far go, and places are found for those it is short of.
/// The game spawns them (see `Game::spawn_voxel_mobs`).
pub fn changes(world: &mut World, delta: f32) -> MobChanges {
    let mut changes = MobChanges { spawn: Vec::new(), remove: Vec::new() };
    let mut clock = world.resource::<VoxelMobClock>().cloned().unwrap_or_default();
    clock.wait -= delta;
    if clock.wait > 0.0 {
        world.insert_resource(clock);
        return changes;
    }
    clock.wait = EVERY;
    let Some(player) = player(world) else {
        world.insert_resource(clock);
        return changes;
    };
    let volumes: Vec<(Entity, Vec3, Vec<VoxelMobsData>)> = world
        .query_ref::<(Entity, &crate::voxel_volumes::VoxelVolumeSource, &VoxelVolume, &Transform, Option<&WorldTransform>)>()
        .into_iter()
        .filter(|(_, source, volume, ..)| volume.endless && !source.0.mobs.is_empty())
        .map(|(entity, source, _, local, placed)| {
            (entity, corner_of(local, placed), source.0.mobs.clone())
        })
        .collect();
    let spawned: Vec<(Entity, VoxelMobSpawned, Option<Vec3>)> = world
        .query_ref::<(Entity, &VoxelMobSpawned)>()
        .without::<Destroying>()
        .into_iter()
        .map(|(entity, spawned)| {
            let at = world.get::<WorldTransform>(entity).map(|placed| placed.0.w_axis.truncate());
            (entity, *spawned, at)
        })
        .collect();
    let flat = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, 0.0, a.z - b.z).length();
    for (volume, corner, kinds) in volumes {
        let dimension = voxear_scene::dimension_of(world, volume);
        for (k, kind) in kinds.iter().enumerate() {
            let Some(prefab) = kind.prefab.filter(|prefab| !clock.failed.contains(prefab)) else { continue };
            let mut alive = 0;
            for (entity, of, at) in &spawned {
                if of.volume != volume || of.kind != k {
                    continue;
                }
                match at {
                    Some(at) if flat(*at, player) > kind.far.max(kind.near) * 1.2 => changes.remove.push(*entity),
                    _ => alive += 1,
                }
            }
            if alive >= kind.wanted() {
                continue;
            }
            for _ in 0..TRIES {
                let (near, far) = (kind.near.max(0.0), kind.far.max(kind.near));
                let turn = clock.chance() * std::f32::consts::TAU;
                let reach = (near * near + clock.chance() * (far * far - near * near)).sqrt();
                let point = player + Vec3::new(turn.cos() * reach, 0.0, turn.sin() * reach);
                let Some((ground, material)) = standing(world, dimension, point + Vec3::Y * 64.0) else { continue };
                let on = if kind.on.is_empty() { &[VoxelMaterial::Grass][..] } else { &kind.on[..] };
                let areas = world.get::<VoxelVolume>(volume).map(|volume| volume.areas.clone()).unwrap_or_default();
                if !on.contains(&material)
                    || !voxear_render::voxel_areas_allow(
                        &areas,
                        &crate::voxel_volumes::area_rules(&kind.within, &kind.never_within),
                        ground - corner,
                    )
                {
                    continue;
                }
                changes.spawn.push((volume, k, prefab, ground, clock.chance() * 360.0));
                break;
            }
        }
    }
    world.insert_resource(clock);
    changes
}

/// The place a spawned mob is given: on the ground, turned `turn` degrees.
pub fn placed_at(ground: Vec3, turn: f32) -> voxear_scene::Placement {
    voxear_scene::Placement { position: ground + Vec3::Y * 0.05, rotation: Quat::from_rotation_y(turn.to_radians()) }
}
