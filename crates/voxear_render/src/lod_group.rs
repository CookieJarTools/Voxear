//! LOD Groups: an Obj that shows one of several sets of Objs at a time, by
//! how much of the screen it covers or how far it is from the camera, as
//! Unity's LOD Group does. Each level is a list of Objs, each shown with
//! everything inside it, so a level can be simpler meshes or something
//! else entirely.
//!
//! Each view chooses for its own camera, so a Scene view and the Game view
//! can show different levels of the same group at once. The choice hides
//! what is drawn and what lights, not the Objs themselves: their scripts
//! keep running.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Parent, Transform, WorldTransform};

use crate::{
    assets::MeshHandle,
    scene::{Light, MeshRenderer},
};

/// Deeper than any real hierarchy; deeper means a cycle.
const MAX_DEPTH: usize = 256;

/// The size a group with nothing drawn in it is measured at, in metres.
const EMPTY_SIZE: f32 = 1.0;

/// What a LOD Group's levels are measured by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LodMeasure {
    /// How much of the screen's height the group's Objs cover.
    #[default]
    ScreenSize,
    /// How far the camera is from the group's Obj, in metres.
    Distance,
}

/// One level of a LOD Group.
#[derive(Clone, Debug, PartialEq)]
pub struct LodLevel {
    /// The Objs this level shows, each with everything inside it, as
    /// Unity's LOD lists its renderers. None shows nothing.
    pub objs: Vec<Entity>,
    /// Where the level ends. Measured by screen size, the fraction of the
    /// screen's height, from 0 to 1, that the group shows this level down to;
    /// by distance, the metres it shows this level out to.
    pub until: f32,
}

/// Shows one of its levels at a time: the first whose `until` the view has
/// not passed. Past the last, the group shows nothing when `cull` is set,
/// and otherwise keeps showing the last level.
#[derive(Clone, Debug, PartialEq)]
pub struct LodGroup {
    pub measure: LodMeasure,
    pub levels: Vec<LodLevel>,
    pub cull: bool,
}
impl Component for LodGroup {}

impl Default for LodGroup {
    fn default() -> Self {
        Self { measure: LodMeasure::ScreenSize, levels: Vec::new(), cull: true }
    }
}

impl LodGroup {
    /// A group measured by screen size with a level for each Obj, ending at
    /// 60%, 30%, 15% and so on of the screen.
    pub fn of(objs: &[Entity]) -> Self {
        let levels = objs
            .iter()
            .enumerate()
            .map(|(index, &obj)| LodLevel { objs: vec![obj], until: default_until(LodMeasure::ScreenSize, index) })
            .collect();
        Self { levels, ..Self::default() }
    }

    /// A group measured by screen size with a level for each list of Objs,
    /// ending at 60%, 30%, 15% and so on of the screen.
    pub fn of_levels(levels: Vec<Vec<Entity>>) -> Self {
        let levels = levels
            .into_iter()
            .enumerate()
            .map(|(index, objs)| LodLevel { objs, until: default_until(LodMeasure::ScreenSize, index) })
            .collect();
        Self { levels, ..Self::default() }
    }

    /// Puts Objs in a level, taking each out of any other level it was in,
    /// as an Obj shows at one level of a group.
    pub fn put(&mut self, level: usize, objs: &[Entity]) {
        for (_, other) in self.levels.iter_mut().enumerate().filter(|(at, _)| *at != level) {
            other.objs.retain(|obj| !objs.contains(obj));
        }
        let Some(at) = self.levels.get_mut(level) else { return };
        for &obj in objs {
            if !at.objs.contains(&obj) {
                at.objs.push(obj);
            }
        }
    }

    /// The level an Obj is in, if any.
    pub fn level_of(&self, obj: Entity) -> Option<usize> {
        self.levels.iter().position(|level| level.objs.contains(&obj))
    }

    /// The level a view shows, or `None` when the group shows nothing.
    pub fn level_for(&self, view: LodView) -> Option<usize> {
        let within = |level: &LodLevel| match self.measure {
            LodMeasure::ScreenSize => view.size >= level.until,
            LodMeasure::Distance => view.distance <= level.until,
        };
        match self.levels.iter().position(within) {
            Some(level) => Some(level),
            None if self.cull => None,
            None => self.levels.len().checked_sub(1),
        }
    }

    /// The level after the last, where a new one ends: half the screen size
    /// of the one before, or twice the distance.
    pub fn next_until(&self) -> f32 {
        match (self.levels.last(), self.measure) {
            (None, measure) => default_until(measure, 0),
            (Some(last), LodMeasure::ScreenSize) => last.until * 0.5,
            (Some(last), LodMeasure::Distance) => last.until * 2.0,
        }
    }

    /// Changes what the levels are measured by, starting each level's end
    /// again from the defaults.
    pub fn set_measure(&mut self, measure: LodMeasure) {
        if measure == self.measure {
            return;
        }
        self.measure = measure;
        for (index, level) in self.levels.iter_mut().enumerate() {
            level.until = default_until(measure, index);
        }
    }
}

/// Where level `index` ends to begin with: 60% of the screen halving each
/// level, or 20 m doubling.
pub fn default_until(measure: LodMeasure, index: usize) -> f32 {
    let index = index.min(16) as i32;
    match measure {
        LodMeasure::ScreenSize => 0.6 * 0.5f32.powi(index),
        LodMeasure::Distance => 20.0 * 2.0f32.powi(index),
    }
}

/// How a camera sees a group: the fraction of the screen's height its Objs
/// cover, and how far away its own Obj is, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodView {
    pub size: f32,
    pub distance: f32,
}

/// A camera as LOD Groups measure from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodCamera {
    pub position: Vec3,
    /// The tangent of half its vertical field of view.
    pub tan_half_fov: f32,
}

impl LodCamera {
    pub fn new(position: Vec3, fov_y_degrees: f32) -> Self {
        Self { position, tan_half_fov: (fov_y_degrees.to_radians() * 0.5).tan().max(1e-4) }
    }
}

/// What every LOD Group shows to one camera.
#[derive(Debug, Default)]
pub struct LodChoices {
    /// Each group's view and the level it shows.
    pub groups: HashMap<Entity, (LodView, Option<usize>)>,
    /// The Objs with a mesh or a light that are in a level not shown.
    pub hidden: HashSet<Entity>,
}

impl LodChoices {
    /// Chooses for every group. `sphere` gives a mesh's bounding sphere in
    /// its own space, which the groups' sizes are measured from.
    pub fn new(world: &World, camera: LodCamera, sphere: &dyn Fn(MeshHandle) -> Option<(Vec3, f32)>) -> Self {
        let groups: Vec<(Entity, &LodGroup)> =
            world.query_ref::<(Entity, &LodGroup)>().without::<Destroying>().into_iter().collect();
        if groups.is_empty() {
            return Self::default();
        }
        // Which group and level each level's Obj is.
        let mut levels: HashMap<Entity, (usize, usize)> = HashMap::new();
        for (group, (_, lod)) in groups.iter().enumerate() {
            for (level, at) in lod.levels.iter().enumerate() {
                for &obj in &at.objs {
                    levels.entry(obj).or_insert((group, level));
                }
            }
        }

        // What each mesh and light is in, and the box around each group's meshes.
        let mut members: Vec<(Entity, Vec<(usize, usize)>)> = Vec::new();
        let mut boxes: Vec<Option<(Vec3, Vec3)>> = vec![None; groups.len()];
        let renderers = world.query_ref::<(Entity, &MeshRenderer)>().without::<Destroying>();
        for (entity, renderer) in renderers {
            let within = levels_above(world, &levels, entity);
            if within.is_empty() {
                continue;
            }
            if let Some((center, radius)) = sphere(renderer.mesh) {
                let model = matrix(world, entity);
                let reach = radius * max_scale(model);
                let center = model.transform_point3(center);
                for &(group, _) in &within {
                    let (low, high) = boxes[group].get_or_insert((center, center));
                    (*low, *high) = (low.min(center - reach), high.max(center + reach));
                }
            }
            members.push((entity, within));
        }
        for (entity, _) in world.query_ref::<(Entity, &Light)>().without::<Destroying>() {
            let within = levels_above(world, &levels, entity);
            if !within.is_empty() {
                members.push((entity, within));
            }
        }

        let mut choices = Self::default();
        let mut shown = Vec::with_capacity(groups.len());
        for ((entity, lod), bounds) in groups.iter().zip(&boxes) {
            let view = measure(world, *entity, *bounds, camera);
            let level = lod.level_for(view);
            choices.groups.insert(*entity, (view, level));
            shown.push(level);
        }
        choices.hidden = members
            .into_iter()
            .filter(|(_, within)| within.iter().any(|&(group, level)| shown[group] != Some(level)))
            .map(|(entity, _)| entity)
            .collect();
        choices
    }
}

/// How a camera sees a group whose meshes fill `bounds`, a box in the world.
fn measure(world: &World, group: Entity, bounds: Option<(Vec3, Vec3)>, camera: LodCamera) -> LodView {
    let at = matrix(world, group).w_axis.truncate();
    let distance = at.distance(camera.position);
    let (center, size) = match bounds {
        Some((low, high)) => ((low + high) * 0.5, (high - low).max_element()),
        None => (at, EMPTY_SIZE),
    };
    let away = center.distance(camera.position).max(1e-4);
    LodView { size: size / (2.0 * away * camera.tan_half_fov), distance }
}

/// The groups and levels an Obj is in, through its parents: the nearest
/// first, as a level of a group inside another group's level is in both.
fn levels_above(world: &World, levels: &HashMap<Entity, (usize, usize)>, entity: Entity) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut at = Some(entity);
    for _ in 0..MAX_DEPTH {
        let Some(current) = at else { break };
        if let Some(&level) = levels.get(&current) {
            found.push(level);
        }
        at = world.get::<Parent>(current).map(|parent| parent.0);
    }
    found
}

fn matrix(world: &World, entity: Entity) -> Mat4 {
    match (world.get::<WorldTransform>(entity), world.get::<Transform>(entity)) {
        (Some(world), _) => world.0,
        (None, Some(local)) => local.matrix(),
        (None, None) => Mat4::IDENTITY,
    }
}

fn max_scale(model: Mat4) -> f32 {
    model.x_axis.truncate().length().max(model.y_axis.truncate().length()).max(model.z_axis.truncate().length())
}

#[cfg(test)]
mod tests {
    use voxear_scene::Name;

    use super::*;
    use crate::assets::MaterialHandle;

    fn rock(world: &mut World, name: &str, parent: Entity) -> Entity {
        let renderer = MeshRenderer::new(MeshHandle(0), MaterialHandle(0));
        world.spawn((Name(name.into()), Transform::IDENTITY, Parent(parent), renderer))
    }

    /// A rock of three levels at the origin, a metre across.
    fn rocks() -> (World, Entity, [Entity; 3]) {
        let mut world = World::new();
        let group = world.spawn((Name("Rock".into()), Transform::IDENTITY));
        let levels = [rock(&mut world, "High", group), rock(&mut world, "Middle", group), rock(&mut world, "Low", group)];
        world.insert(group, LodGroup::of(&levels));
        voxear_scene::propagate_transforms(&mut world);
        (world, group, levels)
    }

    fn seen_from(world: &World, metres: f32) -> LodChoices {
        let camera = LodCamera::new(Vec3::new(0.0, 0.0, -metres), 60.0);
        LodChoices::new(world, camera, &|_| Some((Vec3::ZERO, 0.5)))
    }

    #[test]
    fn a_group_shows_one_level_by_how_much_of_the_screen_it_covers() {
        let (world, group, [high, middle, low]) = rocks();
        let near = seen_from(&world, 1.0);
        assert_eq!(near.groups[&group].1, Some(0));
        assert!(!near.hidden.contains(&high) && near.hidden.contains(&middle) && near.hidden.contains(&low));

        // A metre at 2 m covers over 40% of a 60° view's height.
        let further = seen_from(&world, 2.0);
        let (view, level) = further.groups[&group];
        assert!((0.3..0.6).contains(&view.size), "{view:?}");
        assert_eq!(level, Some(1));
        assert!(further.hidden.contains(&high) && !further.hidden.contains(&middle));

        let gone = seen_from(&world, 100.0);
        assert_eq!(gone.groups[&group].1, None, "past the last level, the group is culled");
        assert!([high, middle, low].iter().all(|level| gone.hidden.contains(level)));
    }

    #[test]
    fn without_culling_the_last_level_stays() {
        let (mut world, group, [.., low]) = rocks();
        world.get_mut::<LodGroup>(group).expect("a group").cull = false;
        let gone = seen_from(&world, 100.0);
        assert_eq!(gone.groups[&group].1, Some(2));
        assert!(!gone.hidden.contains(&low));
    }

    #[test]
    fn a_group_measured_by_distance_switches_at_its_distances() {
        let (mut world, group, [high, middle, _]) = rocks();
        let lod = world.get_mut::<LodGroup>(group).expect("a group");
        lod.set_measure(LodMeasure::Distance);
        assert_eq!(lod.levels.iter().map(|l| l.until).collect::<Vec<_>>(), [20.0, 40.0, 80.0]);
        assert_eq!(seen_from(&world, 19.0).groups[&group].1, Some(0));
        let middling = seen_from(&world, 30.0);
        let (view, level) = middling.groups[&group];
        assert_eq!(level, Some(1));
        assert!((view.distance - 30.0).abs() < 1e-3);
        assert!(middling.hidden.contains(&high) && !middling.hidden.contains(&middle));
        assert_eq!(seen_from(&world, 81.0).groups[&group].1, None);
    }

    #[test]
    fn what_is_inside_a_level_hides_with_it_and_what_is_not_stays() {
        let (mut world, group, [high, ..]) = rocks();
        let moss = rock(&mut world, "Moss", high);
        let lamp = world.spawn((Transform::IDENTITY, Parent(high), Light::point(Vec3::ONE, 1.0, 3.0)));
        let sign = rock(&mut world, "Sign", group);
        voxear_scene::propagate_transforms(&mut world);
        let far = seen_from(&world, 2.0);
        assert!(far.hidden.contains(&moss) && far.hidden.contains(&lamp), "inside a level not shown");
        assert!(!far.hidden.contains(&sign), "in the group but in no level");
    }

    #[test]
    fn a_world_without_groups_hides_nothing() {
        let mut world = World::new();
        let parent = world.spawn((Transform::IDENTITY,));
        rock(&mut world, "Crate", parent);
        let choices = seen_from(&world, 50.0);
        assert!(choices.groups.is_empty() && choices.hidden.is_empty());
    }
}
