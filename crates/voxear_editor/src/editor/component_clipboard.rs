//! Copy Component and Paste Component Values or As New, from a
//! component's menu in the Inspector, as Unity's: a component's values are
//! copied whole, and pasted over the same component on another Obj, or
//! onto an Obj that has none as a new one. A script's are its field
//! values. A LOD Group names Objs inside its own, so it is not copied.

use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours,
    audio::{AudioListener, AudioSource},
    physics::{CharacterController, Collider, Joint, Rigidbody, Vehicle, Wheel},
    physics2d::{Collider2D, Rigidbody2D},
};
use voxear_render::{Camera, DryArea, FoliageInteraction, Light, MeshRenderer, Ocean, ParticleSystem, Pool, PostVolume, Seasons, Stream, WindZone};
use voxear_scene::BehaviourSpec;

use super::Editor;
use crate::props::Part;

/// A component's values, copied.
#[derive(Clone, Debug)]
pub(super) enum Copied {
    Renderer(MeshRenderer),
    Light(Light),
    Camera(Camera),
    Audio(Box<AudioSource>),
    Listener,
    Collider(Collider),
    Rigidbody(Rigidbody),
    Character(CharacterController),
    Joint(Joint),
    Collider2D(Collider2D),
    Rigidbody2D(Rigidbody2D),
    Vehicle(Vehicle),
    Wheel(Wheel),
    PostVolume(Box<PostVolume>),
    WindZone(WindZone),
    ParticleSystem(Box<ParticleSystem>),
    Smoke(voxear_render::Smoke),
    Mist(voxear_render::Mist),
    Animator(voxear_engine::animation::Animator),
    Ocean(Ocean),
    Pool(Pool),
    DryArea(DryArea),
    Path(voxear_scene::Path),
    Stream(Stream),
    Buoyancy(voxear_engine::water::Buoyancy),
    Seasons(Box<Seasons>),
    FoliageInteraction(FoliageInteraction),
    Behaviour(BehaviourSpec),
}

impl Copied {
    /// The component of an Obj, when it has it and it can be copied.
    fn of(world: &World, entity: Entity, part: &Part) -> Option<Self> {
        Some(match part {
            Part::Renderer => Self::Renderer(*world.get::<MeshRenderer>(entity)?),
            Part::Light => Self::Light(*world.get::<Light>(entity)?),
            Part::Camera => Self::Camera(*world.get::<Camera>(entity)?),
            Part::Audio => Self::Audio(Box::new(*world.get::<AudioSource>(entity)?)),
            Part::Listener => world.has::<AudioListener>(entity).then_some(Self::Listener)?,
            // A LOD Group's levels and a Foliage's plants belong where they are.
            Part::LodGroup | Part::Foliage | Part::Model => return None,
            Part::Collider => Self::Collider(*world.get::<Collider>(entity)?),
            Part::Rigidbody => Self::Rigidbody(*world.get::<Rigidbody>(entity)?),
            Part::Character => Self::Character(*world.get::<CharacterController>(entity)?),
            Part::Joint => Self::Joint(*world.get::<Joint>(entity)?),
            Part::Collider2D => Self::Collider2D(*world.get::<Collider2D>(entity)?),
            Part::Rigidbody2D => Self::Rigidbody2D(*world.get::<Rigidbody2D>(entity)?),
            Part::Vehicle => Self::Vehicle(*world.get::<Vehicle>(entity)?),
            Part::Wheel => Self::Wheel(*world.get::<Wheel>(entity)?),
            Part::PostVolume => Self::PostVolume(Box::new(world.get::<PostVolume>(entity)?.clone())),
            Part::WindZone => Self::WindZone(*world.get::<WindZone>(entity)?),
            Part::ParticleSystem => Self::ParticleSystem(Box::new(*world.get::<ParticleSystem>(entity)?)),
            Part::Smoke => Self::Smoke(*world.get::<voxear_render::Smoke>(entity)?),
            Part::Mist => Self::Mist(*world.get::<voxear_render::Mist>(entity)?),
            Part::Animator => Self::Animator(world.get::<voxear_engine::animation::Animator>(entity)?.clone()),
            Part::Ocean => Self::Ocean(*world.get::<Ocean>(entity)?),
            Part::Pool => Self::Pool(*world.get::<Pool>(entity)?),
            Part::DryArea => Self::DryArea(*world.get::<DryArea>(entity)?),
            Part::Path => Self::Path(world.get::<voxear_scene::Path>(entity)?.clone()),
            Part::Stream => Self::Stream(*world.get::<Stream>(entity)?),
            Part::Buoyancy => Self::Buoyancy(*world.get::<voxear_engine::water::Buoyancy>(entity)?),
            Part::Seasons => Self::Seasons(Box::new(*world.get::<Seasons>(entity)?)),
            Part::FoliageInteraction => Self::FoliageInteraction(*world.get::<FoliageInteraction>(entity)?),
            Part::Behaviour(name) => {
                let specs = &world.get::<EditBehaviours>(entity)?.0;
                Self::Behaviour(specs.iter().find(|spec| spec.type_name == *name)?.clone())
            }
        })
    }

    /// Puts the values on an Obj, over its own component or as a new one.
    fn put(&self, world: &mut World, entity: Entity) {
        match self {
            Self::Renderer(renderer) => drop(world.insert(entity, *renderer)),
            Self::Light(light) => drop(world.insert(entity, *light)),
            Self::Camera(camera) => drop(world.insert(entity, *camera)),
            Self::PostVolume(volume) => drop(world.insert(entity, (**volume).clone())),
            Self::WindZone(wind) => drop(world.insert(entity, *wind)),
            Self::ParticleSystem(system) => drop(world.insert(entity, **system)),
            Self::Smoke(smoke) => drop(world.insert(entity, *smoke)),
            Self::Mist(mist) => drop(world.insert(entity, *mist)),
            Self::Animator(animator) => drop(world.insert(entity, animator.clone())),
            Self::Ocean(ocean) => drop(world.insert(entity, *ocean)),
            Self::Pool(pool) => drop(world.insert(entity, *pool)),
            Self::DryArea(dry) => drop(world.insert(entity, *dry)),
            Self::Path(path) => drop(world.insert(entity, path.clone())),
            Self::Stream(stream) => drop(world.insert(entity, *stream)),
            Self::Buoyancy(buoyancy) => drop(world.insert(entity, *buoyancy)),
            Self::Seasons(seasons) => drop(world.insert(entity, **seasons)),
            Self::FoliageInteraction(push) => drop(world.insert(entity, *push)),
            Self::Audio(audio) => drop(world.insert(entity, **audio)),
            Self::Listener => drop(world.insert(entity, AudioListener)),
            Self::Collider(collider) => drop(world.insert(entity, *collider)),
            Self::Rigidbody(body) => drop(world.insert(entity, *body)),
            Self::Character(character) => drop(world.insert(entity, *character)),
            // Held to the Obj it was, unless that is the Obj it goes on.
            Self::Joint(joint) => {
                let connected = joint.connected.filter(|&other| other != entity);
                world.insert(entity, Joint { connected, ..*joint });
            }
            Self::Collider2D(collider) => drop(world.insert(entity, *collider)),
            Self::Rigidbody2D(body) => drop(world.insert(entity, *body)),
            Self::Vehicle(vehicle) => drop(world.insert(entity, *vehicle)),
            Self::Wheel(wheel) => drop(world.insert(entity, *wheel)),
            Self::Behaviour(copied) => match world.get_mut::<EditBehaviours>(entity) {
                Some(kept) => match kept.0.iter_mut().find(|spec| spec.type_name == copied.type_name) {
                    Some(spec) => spec.fields = copied.fields.clone(),
                    None => kept.0.push(copied.clone()),
                },
                None => drop(world.insert(entity, EditBehaviours(vec![copied.clone()]))),
            },
        }
    }
}

impl Editor {
    /// Copies a component of an Obj, for pasting on another.
    pub(super) fn copy_component(&mut self, entity: Entity, part: &Part) {
        let Some(copied) = Copied::of(&self.game.engine.world, entity, part) else {
            return self.notify(&format!("{} cannot be copied", part.title()));
        };
        self.copied = Some(copied);
        for inspector in self.inspectors_mut() {
            inspector.set_copied(Some(part.clone()));
        }
        self.notify(&format!("Copied {}", part.title()));
    }

    /// Pastes the component copied onto an Obj, over its own or as a new
    /// one. The caller records the step.
    pub(super) fn paste_component(&mut self, entity: Entity) {
        if let Some(copied) = &self.copied {
            copied.put(&mut self.game.engine.world, entity);
        }
    }
}
