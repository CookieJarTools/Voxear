//! Engine services that native callbacks reach while scripts run, beyond
//! the world: loading and instantiating prefabs. The engine implements
//! [`ScriptServices`] over its content and lends it for each frame.

use std::{cell::Cell, marker::PhantomData, ptr::NonNull};

use anyhow::Result;
use voxear_ecs::{Entity, World};
use voxear_scene::Placement;

pub trait ScriptServices {
    /// Loads a prefab by GUID or project path. Loading it again returns the
    /// same id.
    fn load_prefab(&mut self, reference: &str) -> Result<u32>;

    /// Creates a loaded prefab's Objs, without behaviours. The root goes
    /// under `parent`, and takes `placement`, relative to it, when it is
    /// given.
    fn spawn_prefab(
        &mut self,
        world: &mut World,
        prefab: u32,
        parent: Option<Entity>,
        placement: Option<Placement>,
    ) -> Result<SpawnedPrefab>;
}

/// What [`ScriptServices::spawn_prefab`] created.
pub struct SpawnedPrefab {
    pub root: Entity,
    /// Every new Obj, with the behaviour types to create on it.
    pub objs: Vec<(Entity, Vec<String>)>,
}

thread_local! {
    static SERVICES: Cell<Option<NonNull<dyn ScriptServices>>> = const { Cell::new(None) };
}

/// Lends services to native callbacks until it is dropped.
pub struct ServicesScope<'a> {
    previous: Option<NonNull<dyn ScriptServices>>,
    _borrow: PhantomData<&'a mut ()>,
}

impl<'a> ServicesScope<'a> {
    pub fn enter(services: &'a mut (dyn ScriptServices + 'a)) -> Self {
        let pointer = NonNull::from(services);
        // The scope's lifetime keeps the borrow alive for as long as the
        // pointer is reachable, which the type cannot say by itself.
        let pointer: NonNull<dyn ScriptServices> = unsafe { std::mem::transmute(pointer) };
        Self { previous: SERVICES.replace(Some(pointer)), _borrow: PhantomData }
    }
}

impl Drop for ServicesScope<'_> {
    fn drop(&mut self) {
        SERVICES.set(self.previous);
    }
}

/// Runs `f` with the lent services, or returns `None` outside any scope.
pub(crate) fn with_services<R>(f: impl FnOnce(&mut dyn ScriptServices) -> R) -> Option<R> {
    let mut services = SERVICES.get()?;
    Some(f(unsafe { services.as_mut() }))
}
