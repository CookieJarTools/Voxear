//! Switching Objs and their components off, as Unity's
//! `GameObject.SetActive(false)` and a component's `enabled = false` do.
//!
//! An Obj marked [`Inactive`] is off, and so is everything under it: nothing
//! of theirs draws, lights, collides or plays, and their behaviours get
//! OnDisable. A component listed in an Obj's [`OffComponents`] is off on its
//! own, the rest of the Obj going on as before. Either way the components
//! stay on the Obj with their values, to switch back on as they were.
//!
//! Systems ask a [`Switched`], taken once per pass, which also knows the
//! Objs that are off because one above them is.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use voxear_ecs::{Component, Entity, World};

use crate::hierarchy::Parent;

/// An engine component that can be switched off, as a scene file names it.
/// The order is part of the C# ABI (`Voxear.EngineComponent`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Switch {
    Renderer,
    Model,
    Light,
    Camera,
    Audio,
    AudioListener,
    LodGroup,
    Collider,
    Character,
    Collider2d,
    Vehicle,
    Wheel,
    PostVolume,
    WindZone,
    Seasons,
    FoliageInteraction,
    Foliage,
}

impl Switch {
    pub const ALL: [Switch; 17] = [
        Switch::Renderer,
        Switch::Model,
        Switch::Light,
        Switch::Camera,
        Switch::Audio,
        Switch::AudioListener,
        Switch::LodGroup,
        Switch::Collider,
        Switch::Character,
        Switch::Collider2d,
        Switch::Vehicle,
        Switch::Wheel,
        Switch::PostVolume,
        Switch::WindZone,
        Switch::Seasons,
        Switch::FoliageInteraction,
        Switch::Foliage,
    ];

    /// The component by its number, as C# passes it.
    pub fn from_index(index: u32) -> Option<Switch> {
        Switch::ALL.get(index as usize).copied()
    }

    /// Its name in a scene file's `enabled` table, beside behaviours' types.
    pub const fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// The component a scene file's `enabled` table names; none for a
    /// behaviour's type.
    pub fn named(name: &str) -> Option<Switch> {
        NAMES.iter().position(|&known| known == name).map(|index| Switch::ALL[index])
    }

    const fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// Each switch's name, as serde writes it, in `Switch::ALL`'s order.
const NAMES: [&str; 17] = [
    "renderer",
    "model",
    "light",
    "camera",
    "audio",
    "audio_listener",
    "lod_group",
    "collider",
    "character",
    "collider2d",
    "vehicle",
    "wheel",
    "post_volume",
    "wind_zone",
    "seasons",
    "foliage_interaction",
    "foliage",
];

/// The components switched off on an Obj. An Obj with every component on
/// has none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OffComponents(u32);
impl Component for OffComponents {}

impl OffComponents {
    pub fn contains(self, switch: Switch) -> bool {
        self.0 & switch.bit() != 0
    }

    pub fn set(&mut self, switch: Switch, off: bool) {
        match off {
            true => self.0 |= switch.bit(),
            false => self.0 &= !switch.bit(),
        }
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Switch> {
        Switch::ALL.into_iter().filter(move |&switch| self.contains(switch))
    }
}

impl FromIterator<Switch> for OffComponents {
    fn from_iter<I: IntoIterator<Item = Switch>>(switches: I) -> Self {
        let mut off = OffComponents::default();
        switches.into_iter().for_each(|switch| off.set(switch, true));
        off
    }
}

/// The Obj is switched off, and everything under it with it, as Unity's
/// `SetActive(false)`. Unity's `activeSelf`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inactive;
impl Component for Inactive {}

/// Whether the Obj's own switch is on, Unity's `activeSelf`.
pub fn is_active_self(world: &World, entity: Entity) -> bool {
    !world.has::<Inactive>(entity)
}

/// Whether the Obj and every Obj above it are on, Unity's
/// `activeInHierarchy`.
pub fn is_active(world: &World, entity: Entity) -> bool {
    let mut current = Some(entity);
    for _ in 0..MAX_DEPTH {
        let Some(obj) = current else { return true };
        if world.has::<Inactive>(obj) {
            return false;
        }
        current = world.get::<Parent>(obj).map(|parent| parent.0);
    }
    true
}

/// Whether a component of the Obj does anything: it is on, and so is its
/// Obj. True for a component the Obj does not have, as there is nothing
/// switched off.
pub fn is_on(world: &World, entity: Entity, switch: Switch) -> bool {
    is_component_on(world, entity, switch) && is_active(world, entity)
}

/// Whether the component's own switch is on, Unity's `enabled`.
pub fn is_component_on(world: &World, entity: Entity, switch: Switch) -> bool {
    world.get::<OffComponents>(entity).is_none_or(|off| !off.contains(switch))
}

/// Switches a component of the Obj on or off. An Obj left with every
/// component on loses its [`OffComponents`].
pub fn set_component_on(world: &mut World, entity: Entity, switch: Switch, on: bool) {
    let mut off = world.get::<OffComponents>(entity).copied().unwrap_or_default();
    if off.contains(switch) != on {
        return;
    }
    off.set(switch, !on);
    match off.is_empty() {
        true => drop(world.remove::<OffComponents>(entity)),
        false => drop(world.insert(entity, off)),
    }
}

/// Switches the Obj on or off, and so everything under it.
pub fn set_active(world: &mut World, entity: Entity, active: bool) {
    match active {
        true => drop(world.remove::<Inactive>(entity)),
        false if !world.has::<Inactive>(entity) => drop(world.insert(entity, Inactive)),
        false => {}
    }
}

/// What is switched off in a world as it is now, for a system to skip: the
/// Objs that are off, by their own switch or one above them.
#[derive(Clone, Debug, Default)]
pub struct Switched {
    inactive: HashSet<Entity>,
}

impl Switched {
    pub fn of(world: &World) -> Self {
        Self { inactive: inactive_objs(world) }
    }

    /// Whether the Obj is on, as [`is_active`].
    pub fn is_active(&self, entity: Entity) -> bool {
        !self.inactive.contains(&entity)
    }

    /// Whether a component of the Obj does anything, as [`is_on`].
    pub fn is_on(&self, world: &World, entity: Entity, switch: Switch) -> bool {
        self.is_active(entity) && is_component_on(world, entity, switch)
    }

    /// The Objs that are off.
    pub fn inactive(&self) -> &HashSet<Entity> {
        &self.inactive
    }
}

/// Every Obj that is off: each [`Inactive`] one and everything under it.
pub fn inactive_objs(world: &World) -> HashSet<Entity> {
    let mut off: Vec<Entity> = world.query_ref::<Entity>().with::<Inactive>().into_iter().collect();
    if off.is_empty() {
        return HashSet::new();
    }
    let mut children: HashMap<Entity, Vec<Entity>> = HashMap::new();
    for (child, parent) in world.query_ref::<(Entity, &Parent)>() {
        children.entry(parent.0).or_default().push(child);
    }
    let mut inactive = HashSet::new();
    while let Some(obj) = off.pop() {
        if inactive.insert(obj) {
            off.extend(children.get(&obj).into_iter().flatten().copied());
        }
    }
    inactive
}

/// Deep enough for any real hierarchy; deeper means a cycle.
const MAX_DEPTH: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Transform;

    #[test]
    fn each_switch_is_named_as_serde_writes_it() {
        for switch in Switch::ALL {
            let written = toml::Value::try_from(switch).unwrap();
            assert_eq!(written.as_str(), Some(switch.name()));
            assert_eq!(Switch::named(switch.name()), Some(switch));
        }
        assert_eq!(Switch::named("Game.Spinner"), None);
    }

    #[test]
    fn an_obj_switched_off_takes_everything_under_it_with_it() {
        let mut world = World::new();
        let root = world.spawn(Transform::IDENTITY);
        let child = world.spawn((Transform::IDENTITY, Parent(root)));
        let grandchild = world.spawn((Transform::IDENTITY, Parent(child)));
        let other = world.spawn(Transform::IDENTITY);
        assert!(Switched::of(&world).inactive().is_empty());

        set_active(&mut world, child, false);
        let switched = Switched::of(&world);
        assert!(switched.is_active(root) && switched.is_active(other));
        assert!(!switched.is_active(child) && !switched.is_active(grandchild));
        assert!(!is_active(&world, grandchild) && is_active_self(&world, grandchild));

        set_active(&mut world, child, true);
        assert!(is_active(&world, grandchild));
    }

    #[test]
    fn a_component_switched_off_leaves_the_others_on() {
        let mut world = World::new();
        let obj = world.spawn(Transform::IDENTITY);
        set_component_on(&mut world, obj, Switch::Light, false);
        assert!(!is_on(&world, obj, Switch::Light));
        assert!(is_on(&world, obj, Switch::Renderer));
        set_active(&mut world, obj, false);
        assert!(!is_on(&world, obj, Switch::Renderer));
        set_component_on(&mut world, obj, Switch::Light, true);
        assert!(!world.has::<OffComponents>(obj));
    }
}
