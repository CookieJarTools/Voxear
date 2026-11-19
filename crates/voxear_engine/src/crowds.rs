//! Crowds: how much of the frame each character that finds its way or is
//! animated gets, by how near it is to whoever is looking.
//!
//! Every Obj with a Nav Agent, a Block Mob, a Mob Motion, a Motion Matcher
//! or an Animator is in a **tier** each frame, by its distance to the nearest
//! viewpoint: the game's camera (or the terrain's viewer), and on a server
//! each player's Obj too. Near, everything is done every step and frame, as
//! it always was. In the middle distance an agent plans its way less often
//! and is posed less often, and a Motion Matcher searches less often and
//! does not hold its feet to the ground. Far, it also moves less often, in
//! larger steps. With no viewpoint at all, as in a headless test, every Obj
//! is near.
//!
//! The easy way is to do nothing: Project Settings ▸ Crowds has the
//! distances and how often each tier does what. The full way is a
//! [`CrowdMember`] on an Obj: a tier of its own, or an importance that
//! scales the distances it is measured by.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// How much of the frame a character gets, by how near it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrowdTier {
    /// Within the near distance: everything every step and frame.
    #[default]
    Near,
    /// Out to the middle distance: planned and posed less often.
    Middle,
    /// Beyond it: planned, moved and posed less often still.
    Far,
}

/// The tier a Crowd Member is in: by its distance, or always one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrowdTierChoice {
    #[default]
    ByDistance,
    Near,
    Middle,
    Far,
}

/// Puts an Obj's place in a crowd in its own hands: a tier it is always in,
/// or how much it matters, which scales the distances its tier is found by.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CrowdMember {
    /// Its tier by its distance, or always this one.
    pub tier: CrowdTierChoice,
    /// How much it matters: 2 stays near twice as far out, 0.5 half as far.
    pub importance: f32,
    /// Becoming a record beyond Project Settings ▸ Crowds ▸ Records beyond,
    /// its Obj gone until it comes near again (see `crowd_records`): for a
    /// root made from a prefab, in the level's dimension.
    pub keep_as_record: bool,
}

impl Default for CrowdMember {
    fn default() -> Self {
        Self { tier: CrowdTierChoice::ByDistance, importance: 1.0, keep_as_record: false }
    }
}

impl Component for CrowdMember {}

/// Project Settings ▸ Crowds, `[crowds]` in `voxear.toml`:
///
/// ```toml
/// [crowds]
/// near = 40.0              # metres from a viewpoint within which all is done every step
/// middle = 150.0           # metres out to which the middle tier reaches
/// middle_plan_every = 2    # steps between an agent's plans in the middle distance
/// far_plan_every = 4
/// far_move_every = 2       # steps between a far agent's moves, each the longer
/// middle_pose_every = 2    # frames between poses in the middle distance
/// far_pose_every = 4
/// middle_search_every = 3.0  # a Motion Matcher's time between searches, times this
/// records_beyond = 250.0   # metres beyond which Crowd Members kept as records go; 0 for never
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CrowdSettings {
    /// Off, every Obj is near.
    pub enabled: bool,
    pub near: f32,
    pub middle: f32,
    pub middle_plan_every: u32,
    pub far_plan_every: u32,
    pub far_move_every: u32,
    pub middle_pose_every: u32,
    pub far_pose_every: u32,
    pub middle_search_every: f32,
    pub records_beyond: f32,
}

impl Default for CrowdSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            near: 40.0,
            middle: 150.0,
            middle_plan_every: 2,
            far_plan_every: 4,
            far_move_every: 2,
            middle_pose_every: 2,
            far_pose_every: 4,
            middle_search_every: 3.0,
            records_beyond: 250.0,
        }
    }
}

impl CrowdSettings {
    /// These settings kept within what makes sense.
    pub fn bounded(&self) -> Self {
        let every = |n: u32| n.clamp(1, 60);
        let near = if self.near.is_finite() { self.near.clamp(1.0, 10_000.0) } else { 40.0 };
        let middle = if self.middle.is_finite() { self.middle.clamp(near, 100_000.0) } else { near.max(150.0) };
        Self {
            enabled: self.enabled,
            near,
            middle,
            middle_plan_every: every(self.middle_plan_every),
            far_plan_every: every(self.far_plan_every),
            far_move_every: every(self.far_move_every),
            middle_pose_every: every(self.middle_pose_every),
            far_pose_every: every(self.far_pose_every),
            middle_search_every: if self.middle_search_every.is_finite() {
                self.middle_search_every.clamp(1.0, 20.0)
            } else {
                3.0
            },
            // Past where the middle tier ends, and past the margin records come back within.
            records_beyond: match self.records_beyond.is_finite() && self.records_beyond > 0.0 {
                true => self.records_beyond.clamp(middle + crate::crowd_records::RECORD_MARGIN * 2.0, 1_000_000.0),
                false => 0.0,
            },
        }
    }

    /// Puts these settings in `world`, for the crowd from the next frame.
    pub fn put(&self, world: &mut World) {
        world.insert_resource(self.bounded());
    }

    /// Steps between an agent's plans in `tier`.
    pub fn plan_every(&self, tier: CrowdTier) -> u32 {
        match tier {
            CrowdTier::Near => 1,
            CrowdTier::Middle => self.middle_plan_every,
            CrowdTier::Far => self.far_plan_every,
        }
    }

    /// Steps between an agent's moves in `tier`.
    pub fn move_every(&self, tier: CrowdTier) -> u32 {
        match tier {
            CrowdTier::Far => self.far_move_every,
            _ => 1,
        }
    }

    /// Frames between a character's poses in `tier`.
    pub fn pose_every(&self, tier: CrowdTier) -> u32 {
        match tier {
            CrowdTier::Near => 1,
            CrowdTier::Middle => self.middle_pose_every,
            CrowdTier::Far => self.far_pose_every,
        }
    }
}

/// Each crowd Obj's tier this frame, how many there are in each, and the
/// frames and steps counted, which spread the work of each tier over them.
#[derive(Clone, Debug, Default)]
pub struct CrowdTiers {
    tiers: HashMap<Entity, CrowdTier>,
    /// How many Objs are in each tier: near, middle, far.
    pub counts: [u32; 3],
    pub frame: u64,
    pub step: u64,
}

impl CrowdTiers {
    /// An Obj's tier: near where it has none.
    pub fn of(&self, entity: Entity) -> CrowdTier {
        self.tiers.get(&entity).copied().unwrap_or_default()
    }
}

/// Whether an Obj does what it does every `every` frames or steps on
/// `count`: spread by its index, so a crowd's work is shared out evenly.
pub fn turn(every: u32, count: u64, entity: Entity) -> bool {
    every <= 1 || (count + u64::from(entity.index())) % u64::from(every) == 0
}

/// An Obj's tier this frame, and the settings the tiers were found by.
pub fn tier_of(world: &World, entity: Entity) -> (CrowdTier, CrowdSettings) {
    let settings = world.resource::<CrowdSettings>().cloned().unwrap_or_default();
    let tier = world.resource::<CrowdTiers>().map_or(CrowdTier::Near, |tiers| tiers.of(entity));
    (tier, settings)
}

/// Where the crowd is measured from: the game's camera, or the terrain's
/// viewer, and on a server each player's Obj.
fn viewpoints(world: &World) -> Vec<Vec3> {
    let mut points: Vec<Vec3> = crate::terrain::viewer(world).into_iter().collect();
    points.extend(crate::net::player_viewpoints(world));
    points
}

fn position(world: &World, entity: Entity) -> Option<Vec3> {
    world
        .get::<WorldTransform>(entity)
        .map(|placed| placed.0.w_axis.truncate())
        .or_else(|| world.get::<Transform>(entity).map(|transform| transform.position))
}

/// Finds each crowd Obj's tier for this frame, once a frame before its
/// fixed steps.
pub fn update(world: &mut World) {
    let settings = world.resource::<CrowdSettings>().cloned().unwrap_or_default();
    let points = viewpoints(world);
    let mut members: Vec<Entity> = Vec::new();
    {
        let mut add = |entity: Entity| members.push(entity);
        world
            .query_ref::<(Entity, &crate::nav_agent::NavAgent)>()
            .without::<Destroying>()
            .into_iter()
            .for_each(|(e, _)| add(e));
        world
            .query_ref::<(Entity, &crate::block_mob::BlockMob)>()
            .without::<Destroying>()
            .into_iter()
            .for_each(|(e, _)| add(e));
        world
            .query_ref::<(Entity, &crate::animation::MobMotion)>()
            .without::<Destroying>()
            .into_iter()
            .for_each(|(e, _)| add(e));
        world
            .query_ref::<(Entity, &crate::animation::MotionMatcher)>()
            .without::<Destroying>()
            .into_iter()
            .for_each(|(e, _)| add(e));
        world
            .query_ref::<(Entity, &crate::animation::Animator)>()
            .without::<Destroying>()
            .into_iter()
            .for_each(|(e, _)| add(e));
    }
    members.sort_unstable();
    members.dedup();
    let mut tiers = HashMap::with_capacity(members.len());
    let mut counts = [0u32; 3];
    for entity in members {
        let member = world.get::<CrowdMember>(entity).copied().unwrap_or_default();
        let tier = match member.tier {
            CrowdTierChoice::Near => CrowdTier::Near,
            CrowdTierChoice::Middle => CrowdTier::Middle,
            CrowdTierChoice::Far => CrowdTier::Far,
            CrowdTierChoice::ByDistance if !settings.enabled || points.is_empty() => CrowdTier::Near,
            CrowdTierChoice::ByDistance => {
                let Some(at) = position(world, entity) else { continue };
                let nearest = points.iter().map(|point| point.distance(at)).fold(f32::INFINITY, f32::min);
                let scale = if member.importance.is_finite() { member.importance.max(0.01) } else { 1.0 };
                match nearest / scale {
                    d if d <= settings.near => CrowdTier::Near,
                    d if d <= settings.middle => CrowdTier::Middle,
                    _ => CrowdTier::Far,
                }
            }
        };
        counts[tier as usize] += 1;
        tiers.insert(entity, tier);
    }
    let (frame, step) = world.resource::<CrowdTiers>().map_or((0, 0), |old| (old.frame + 1, old.step));
    world.insert_resource(CrowdTiers { tiers, counts, frame, step });
}

/// Counts a fixed step, for agents' turns.
pub(crate) fn count_step(world: &mut World) -> u64 {
    match world.resource_mut::<CrowdTiers>() {
        Some(tiers) => {
            tiers.step += 1;
            tiers.step
        }
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_are_shared_out_over_the_crowd() {
        let mut world = World::new();
        let crowd: Vec<Entity> = (0..8).map(|_| world.spawn(())).collect();
        for count in 0..4 {
            let taking = crowd.iter().filter(|entity| turn(4, count, **entity)).count();
            assert_eq!(taking, 2, "a quarter of them on each count");
        }
        assert!(crowd.iter().all(|entity| turn(1, 7, *entity)), "every one, every time");
    }

    #[test]
    fn settings_are_kept_within_what_makes_sense() {
        let wild = CrowdSettings {
            near: -5.0,
            middle: 2.0,
            far_plan_every: 0,
            middle_search_every: f32::NAN,
            ..Default::default()
        };
        let kept = wild.bounded();
        assert_eq!(kept.near, 1.0);
        assert_eq!(kept.middle, 2.0);
        assert_eq!(kept.far_plan_every, 1);
        assert_eq!(kept.middle_search_every, 3.0);
    }
}
