//! Undo and redo for edits made in the editor: an Obj's name, Transform,
//! persistence or components changed, Objs made or deleted, and Objs moved to another
//! parent. Typing
//! into one field records one step, not one per key, as changes to the
//! same Obj close together merge. Objs that undo or redo bring back are new
//! entities, so every step naming them is pointed at the new ones.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use voxear_ecs::{Component, Entity, World};
use voxear_engine::{
    EditBehaviours, Game,
    audio::{AudioListener, AudioSource},
    physics::{CharacterController, Collider, Joint, Rigidbody, Vehicle, Wheel},
    physics2d::{Collider2D, Rigidbody2D},
    prefab::{FromPrefab, Locks, PrefabInstance},
    scene_file::{FoliageSource, ModelSource},
};
use voxear_render::{
    Camera, Foliage, FoliageInteraction, Light, LodGroup, MeshRenderer, ModelRenderer, Ocean, ParticleSystem, Renderer, Seasons, WindZone, DryArea, Pool, Stream,
};
use voxear_scene::{Layer, Name, ScenePersistence, Transform};

use crate::objs::{self, Placing, Taken};

/// Edits to the same Obj closer together than this are one step.
const MERGE_WITHIN: Duration = Duration::from_millis(800);

/// What a change edits about an Obj: its name, its Transform, whether it
/// stays through level changes, and the components the Inspector edits,
/// each there or not. Its scripts are kept as data in edit mode; while
/// playing nothing is undone.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjState {
    pub name: String,
    pub transform: Transform,
    pub persistent: bool,
    pub layer: Option<Layer>,
    pub renderer: Option<MeshRenderer>,
    /// Lights and Audio Sources are boxed: few Objs have one, and every
    /// undo step holds two states.
    pub light: Option<Box<Light>>,
    pub camera: Option<Camera>,
    pub audio: Option<Box<AudioSource>>,
    pub listener: bool,
    pub lod_group: Option<Box<LodGroup>>,
    /// Physics is boxed too, being on few Objs.
    pub collider: Option<Box<Collider>>,
    pub rigidbody: Option<Box<Rigidbody>>,
    pub character: Option<Box<CharacterController>>,
    pub wind_zone: Option<Box<WindZone>>,
    pub particles: Option<Box<ParticleSystem>>,
    pub smoke: Option<Box<voxear_render::Smoke>>,
    pub mist: Option<Box<voxear_render::Mist>>,
    pub animator: Option<Box<voxear_engine::animation::Animator>>,
    pub motion_matcher: Option<Box<voxear_engine::animation::MotionMatcher>>,
    pub bone_attachment: Option<Box<voxear_engine::animation::BoneAttachment>>,
    pub ocean: Option<Box<Ocean>>,
    pub pool: Option<Box<Pool>>,
    pub dry_area: Option<Box<DryArea>>,
    pub path: Option<Box<voxear_scene::Path>>,
    pub stream: Option<Box<Stream>>,
    pub buoyancy: Option<Box<voxear_engine::water::Buoyancy>>,
    pub seasons: Option<Box<Seasons>>,
    pub foliage_interaction: Option<FoliageInteraction>,
    /// A Foliage and how it was written; its plants are shared, not copied.
    pub foliage: Option<Box<(Foliage, FoliageSource)>>,
    /// A Model Renderer and how it was written; its parts are shared.
    pub model: Option<Box<(ModelRenderer, ModelSource)>>,
    pub joint: Option<Box<Joint>>,
    pub collider2d: Option<Box<Collider2D>>,
    pub rigidbody2d: Option<Box<Rigidbody2D>>,
    pub vehicle: Option<Box<Vehicle>>,
    pub wheel: Option<Box<Wheel>>,
    pub behaviours: Option<EditBehaviours>,
    /// Components locked for designers, when this is a prefab's Obj.
    pub locks: Option<Locks>,
}

impl ObjState {
    pub fn of(world: &World, entity: Entity) -> Option<Self> {
        Some(Self {
            name: world.get::<Name>(entity)?.0.clone(),
            transform: *world.get::<Transform>(entity)?,
            persistent: world.has::<ScenePersistence>(entity),
            layer: world.get::<Layer>(entity).copied(),
            renderer: world.get::<MeshRenderer>(entity).copied(),
            light: world.get::<Light>(entity).copied().map(Box::new),
            camera: world.get::<Camera>(entity).copied(),
            audio: world.get::<AudioSource>(entity).copied().map(Box::new),
            listener: world.has::<AudioListener>(entity),
            lod_group: world.get::<LodGroup>(entity).cloned().map(Box::new),
            collider: world.get::<Collider>(entity).copied().map(Box::new),
            rigidbody: world.get::<Rigidbody>(entity).copied().map(Box::new),
            character: world.get::<CharacterController>(entity).copied().map(Box::new),
            wind_zone: world.get::<WindZone>(entity).copied().map(Box::new),
            particles: world.get::<ParticleSystem>(entity).copied().map(Box::new),
            smoke: world.get::<voxear_render::Smoke>(entity).copied().map(Box::new),
            mist: world.get::<voxear_render::Mist>(entity).copied().map(Box::new),
            animator: world.get::<voxear_engine::animation::Animator>(entity).cloned().map(Box::new),
            motion_matcher: world.get::<voxear_engine::animation::MotionMatcher>(entity).cloned().map(Box::new),
            bone_attachment: world.get::<voxear_engine::animation::BoneAttachment>(entity).cloned().map(Box::new),
            ocean: world.get::<Ocean>(entity).copied().map(Box::new),
            pool: world.get::<Pool>(entity).copied().map(Box::new),
            dry_area: world.get::<DryArea>(entity).copied().map(Box::new),
            path: world.get::<voxear_scene::Path>(entity).cloned().map(Box::new),
            stream: world.get::<Stream>(entity).copied().map(Box::new),
            buoyancy: world.get::<voxear_engine::water::Buoyancy>(entity).copied().map(Box::new),
            seasons: world.get::<Seasons>(entity).copied().map(Box::new),
            foliage_interaction: world.get::<FoliageInteraction>(entity).copied(),
            foliage: world
                .get::<Foliage>(entity)
                .cloned()
                .zip(world.get::<FoliageSource>(entity).cloned())
                .map(Box::new),
            model: world
                .get::<ModelRenderer>(entity)
                .cloned()
                .zip(world.get::<ModelSource>(entity).cloned())
                .map(Box::new),
            joint: world.get::<Joint>(entity).copied().map(Box::new),
            collider2d: world.get::<Collider2D>(entity).copied().map(Box::new),
            rigidbody2d: world.get::<Rigidbody2D>(entity).copied().map(Box::new),
            vehicle: world.get::<Vehicle>(entity).copied().map(Box::new),
            wheel: world.get::<Wheel>(entity).copied().map(Box::new),
            behaviours: world.get::<EditBehaviours>(entity).cloned(),
            locks: world.get::<Locks>(entity).cloned(),
        })
    }

    /// Follows Objs brought back as new entities, as its LOD Group's levels
    /// and its joint point at them.
    fn remap(&mut self, renamed: &HashMap<Entity, Entity>) {
        let levels = self.lod_group.iter_mut().flat_map(|group| group.levels.iter_mut());
        for obj in levels.flat_map(|level| level.objs.iter_mut()) {
            *obj = renamed.get(obj).copied().unwrap_or(*obj);
        }
        if let Some(obj) = self.joint.as_mut().and_then(|joint| joint.connected.as_mut()) {
            *obj = renamed.get(obj).copied().unwrap_or(*obj);
        }
    }

    pub fn apply(&self, world: &mut World, entity: Entity) {
        if let Some(name) = world.get_mut::<Name>(entity) {
            name.0.clone_from(&self.name);
        }
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            *transform = self.transform;
        }
        put(world, entity, self.persistent.then_some(ScenePersistence));
        put(world, entity, self.layer);
        put(world, entity, self.renderer);
        put(world, entity, self.light.as_deref().copied());
        put(world, entity, self.camera);
        put(world, entity, self.audio.as_deref().copied());
        put(world, entity, self.listener.then_some(AudioListener));
        put(world, entity, self.lod_group.as_deref().cloned());
        put(world, entity, self.collider.as_deref().copied());
        put(world, entity, self.rigidbody.as_deref().copied());
        put(world, entity, self.character.as_deref().copied());
        put(world, entity, self.wind_zone.as_deref().copied());
        put(world, entity, self.particles.as_deref().copied());
        put(world, entity, self.smoke.as_deref().copied());
        put(world, entity, self.mist.as_deref().copied());
        put(world, entity, self.animator.as_deref().cloned());
        put(world, entity, self.motion_matcher.as_deref().cloned());
        put(world, entity, self.bone_attachment.as_deref().cloned());
        put(world, entity, self.ocean.as_deref().copied());
        put(world, entity, self.pool.as_deref().copied());
        put(world, entity, self.dry_area.as_deref().copied());
        put(world, entity, self.path.as_deref().cloned());
        put(world, entity, self.stream.as_deref().copied());
        put(world, entity, self.buoyancy.as_deref().copied());
        put(world, entity, self.seasons.as_deref().copied());
        put(world, entity, self.foliage_interaction);
        put(world, entity, self.foliage.as_deref().map(|(foliage, _)| foliage.clone()));
        put(world, entity, self.foliage.as_deref().map(|(_, source)| source.clone()));
        put(world, entity, self.model.as_deref().map(|(drawn, _)| drawn.clone()));
        put(world, entity, self.model.as_deref().map(|(_, source)| source.clone()));
        put(world, entity, self.joint.as_deref().copied());
        put(world, entity, self.collider2d.as_deref().copied());
        put(world, entity, self.rigidbody2d.as_deref().copied());
        put(world, entity, self.vehicle.as_deref().copied());
        put(world, entity, self.wheel.as_deref().copied());
        put(world, entity, self.behaviours.clone());
        put(world, entity, self.locks.clone());
    }
}

/// Gives the Obj the component, or takes it away, unless that is how it is.
fn put<T: Component + PartialEq>(world: &mut World, entity: Entity, component: Option<T>) {
    if world.get::<T>(entity) == component.as_ref() {
        return;
    }
    match component {
        Some(component) => {
            world.insert(entity, component);
        }
        None => {
            world.remove::<T>(entity);
        }
    }
}

/// One step.
#[derive(Debug)]
pub enum Edit {
    /// An Obj's name or Transform changed.
    Changed { entity: Entity, before: ObjState, after: ObjState },
    /// Objs made or deleted. Undo and redo each take them away or bring
    /// them back; `taken` holds them while they are away.
    Exists { root: Entity, taken: Option<Taken> },
    /// An Obj moved to another parent.
    Moved { entity: Entity, before: Placing, after: Placing },
    /// Edits that are one step, such as values kept from play.
    Group(Vec<Edit>),
    /// Objs linked to prefab instances otherwise, as unpacking one does.
    Linked(Vec<Link>),
}

/// An Obj's link to a prefab instance, before and after: the instance it
/// is in, and the prefab it is the root of an instance of.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub entity: Entity,
    pub before: (Option<FromPrefab>, Option<PrefabInstance>),
    pub after: (Option<FromPrefab>, Option<PrefabInstance>),
}

impl Link {
    /// Puts the link on, as it is before the change or after.
    pub fn apply(&self, world: &mut World, after: bool) {
        let (from, instance) = if after { &self.after } else { &self.before };
        match from {
            Some(from) => world.insert(self.entity, from.clone()),
            None => world.remove::<FromPrefab>(self.entity).is_some(),
        };
        match instance {
            Some(instance) => world.insert(self.entity, *instance),
            None => world.remove::<PrefabInstance>(self.entity).is_some(),
        };
    }

    fn remap(&mut self, renamed: &HashMap<Entity, Entity>) {
        let follow = |entity: &mut Entity| *entity = renamed.get(entity).copied().unwrap_or(*entity);
        follow(&mut self.entity);
        for from in [&mut self.before.0, &mut self.after.0].into_iter().flatten() {
            follow(&mut from.instance);
        }
    }
}

impl Edit {
    /// Takes the edit back, or makes it again. Returns the Obj to select,
    /// if any, and the new entity of each Obj brought back.
    fn apply(
        &mut self,
        game: &mut Game,
        renderer: Option<&mut Renderer>,
        undo: bool,
    ) -> (Option<Entity>, HashMap<Entity, Entity>) {
        match self {
            Edit::Changed { entity, before, after } => {
                (if undo { before } else { after }).apply(&mut game.engine.world, *entity);
                (Some(*entity), HashMap::new())
            }
            Edit::Exists { root, taken } => match taken.take() {
                Some(away) => match objs::bring_back(game, renderer, &away) {
                    Ok(renamed) => (renamed.get(root).copied(), renamed),
                    Err(error) => {
                        tracing::error!("could not bring the Objs back: {error:#}");
                        *taken = Some(away);
                        (None, HashMap::new())
                    }
                },
                None => {
                    *taken = Some(objs::take(game, *root));
                    (None, HashMap::new())
                }
            },
            Edit::Moved { entity, before, after } => {
                (if undo { before } else { after }).apply(game, *entity);
                (Some(*entity), HashMap::new())
            }
            Edit::Linked(links) => {
                links.iter().for_each(|link| link.apply(&mut game.engine.world, !undo));
                (links.first().map(|link| link.entity), HashMap::new())
            }
            Edit::Group(edits) => {
                let mut renderer = renderer;
                let (mut selected, mut renamed) = (None, HashMap::new());
                let order: Vec<usize> =
                    if undo { (0..edits.len()).rev().collect() } else { (0..edits.len()).collect() };
                for index in order {
                    let (select, more) = edits[index].apply(game, renderer.as_deref_mut(), undo);
                    edits.iter_mut().for_each(|edit| edit.remap(&more));
                    renamed.extend(more);
                    selected = select.or(selected);
                }
                (selected, renamed)
            }
        }
    }

    /// Follows Objs brought back as new entities.
    fn remap(&mut self, renamed: &HashMap<Entity, Entity>) {
        if renamed.is_empty() {
            return;
        }
        let follow = |entity: &mut Entity| *entity = renamed.get(entity).copied().unwrap_or(*entity);
        match self {
            Edit::Changed { entity, before, after } => {
                follow(entity);
                before.remap(renamed);
                after.remap(renamed);
            }
            Edit::Exists { root, taken } => {
                follow(root);
                if let Some(taken) = taken {
                    taken.remap(renamed);
                }
            }
            Edit::Moved { entity, before, after } => {
                follow(entity);
                before.remap(renamed);
                after.remap(renamed);
            }
            Edit::Group(edits) => edits.iter_mut().for_each(|edit| edit.remap(renamed)),
            Edit::Linked(links) => links.iter_mut().for_each(|link| link.remap(renamed)),
        }
    }
}

#[derive(Default)]
pub struct History {
    done: Vec<(Edit, Instant)>,
    undone: Vec<Edit>,
}

impl History {
    /// Notes an edit already made. A change to the same Obj soon after the
    /// last becomes part of it; a new edit means nothing is left to redo.
    pub fn record(&mut self, edit: Edit) {
        self.undone.clear();
        let now = Instant::now();
        if let (Some((Edit::Changed { entity: last, after, .. }, at)), Edit::Changed { entity, after: next, .. }) =
            (self.done.last_mut(), &edit)
            && last == entity
            && now.duration_since(*at) < MERGE_WITHIN
        {
            after.clone_from(next);
            *at = now;
            return;
        }
        self.done.push((edit, now));
    }

    /// Notes an edit that is a step of its own, such as a drag of a handle,
    /// never merged with the one before.
    pub fn record_step(&mut self, edit: Edit) {
        self.undone.clear();
        self.done.push((edit, Instant::now() - MERGE_WITHIN));
    }

    /// Takes back the last edit. `None` when there is none; otherwise the
    /// Obj to select, if any.
    pub fn undo(&mut self, game: &mut Game, renderer: Option<&mut Renderer>) -> Option<Option<Entity>> {
        let (mut edit, _) = self.done.pop()?;
        let (selected, renamed) = edit.apply(game, renderer, true);
        self.undone.push(edit);
        self.remap(&renamed);
        Some(selected)
    }

    /// Makes the last undone edit again. `None` when there is none;
    /// otherwise the Obj to select, if any.
    pub fn redo(&mut self, game: &mut Game, renderer: Option<&mut Renderer>) -> Option<Option<Entity>> {
        let mut edit = self.undone.pop()?;
        let (selected, renamed) = edit.apply(game, renderer, false);
        // Not merged with what came before, so it undoes on its own again.
        self.done.push((edit, Instant::now() - MERGE_WITHIN));
        self.remap(&renamed);
        Some(selected)
    }

    /// Forgets everything, as when another level opens and the Objs go.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }

    fn remap(&mut self, renamed: &HashMap<Entity, Entity>) {
        self.done.iter_mut().for_each(|(edit, _)| edit.remap(renamed));
        self.undone.iter_mut().for_each(|edit| edit.remap(renamed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objs::tests::{game, named};

    fn moved(game: &mut Game, entity: Entity, x: f32, history: &mut History) {
        let world = &mut game.engine.world;
        let before = ObjState::of(world, entity).unwrap();
        world.get_mut::<Transform>(entity).unwrap().position.x = x;
        let after = ObjState::of(world, entity).unwrap();
        history.record(Edit::Changed { entity, before, after });
    }

    #[test]
    fn changes_undo_redo_and_merge_while_typing() {
        let (_dir, mut game) = game();
        let entity = named(&game, "Crate").unwrap();
        let x = |game: &Game| game.engine.world.get::<Transform>(entity).unwrap().position.x;
        let mut history = History::default();
        moved(&mut game, entity, 1.0, &mut history);
        moved(&mut game, entity, 12.0, &mut history);
        assert_eq!(history.undo(&mut game, None), Some(Some(entity)));
        assert_eq!(x(&game), 0.0, "typing 1 then 12 is one step");
        assert_eq!(history.redo(&mut game, None), Some(Some(entity)));
        assert_eq!(x(&game), 12.0);
        assert!(history.undo(&mut game, None).is_some());
        assert!(history.undo(&mut game, None).is_none(), "nothing more to undo");
        moved(&mut game, entity, 3.0, &mut history);
        assert!(history.redo(&mut game, None).is_none(), "a new edit leaves nothing to redo");
    }

    #[test]
    fn steps_follow_objs_that_undo_brings_back() {
        let (_dir, mut game) = game();
        let mut history = History::default();
        let (lid, box_) = (named(&game, "Lid").unwrap(), named(&game, "Box").unwrap());
        moved(&mut game, lid, 5.0, &mut history);
        // The Lid is deleted, then the Box it was in.
        for entity in [lid, box_] {
            let taken = objs::take(&mut game, entity);
            history.record(Edit::Exists { root: entity, taken: Some(taken) });
        }
        assert!(named(&game, "Box").is_none());

        assert_eq!(history.undo(&mut game, None), Some(named(&game, "Box")), "the Box is back, and selected");
        assert!(history.undo(&mut game, None).is_some());
        let (new_lid, new_box) = (named(&game, "Lid").unwrap(), named(&game, "Box").unwrap());
        let parent = game.engine.world.get::<voxear_scene::Parent>(new_lid).map(|p| p.0);
        assert_eq!(parent, Some(new_box), "the Lid went back into the new Box");
        assert_eq!(game.engine.world.get::<Transform>(new_lid).unwrap().position.x, 5.0);
        assert_eq!(history.undo(&mut game, None), Some(Some(new_lid)), "the move before follows the new Lid");
        assert_eq!(game.engine.world.get::<Transform>(new_lid).unwrap().position.x, 0.0);

        assert!(history.redo(&mut game, None).is_some());
        assert!(history.redo(&mut game, None).is_some());
        assert!(named(&game, "Lid").is_none(), "redo deletes it again");
    }
}
