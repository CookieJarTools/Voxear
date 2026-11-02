//! Copy Component and Paste Component Values or As New, from a
//! component's menu in the Inspector, as Unity's: a component's values are
//! copied whole, and pasted over the same component on another Obj, or
//! onto an Obj that has none as a new one. A script's are its field
//! values. A LOD Group names Objs inside its own, so it is not copied.

use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours,
    audio::{AudioListener, AudioSource},
};
use voxear_render::{Camera, Light, MeshRenderer};
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
            Part::LodGroup => return None,
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
            Self::Audio(audio) => drop(world.insert(entity, **audio)),
            Self::Listener => drop(world.insert(entity, AudioListener)),
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
