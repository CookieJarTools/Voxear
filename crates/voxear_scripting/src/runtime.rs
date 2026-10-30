use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, bail, ensure};
use voxear_core::time::Time;
use voxear_ecs::{Entity, World};
use voxear_scene::{BehaviourHandle, BehaviourHost, BehaviourSpec, Callbacks, Phase, Registration, Request};

use crate::{
    hostfxr::{DotnetInstall, Runtime},
    interop::{self, BehaviourInfo, FLAG_INSTANCE, ManagedApi, NativeApi, WorldScope},
};

const BOOTSTRAP_TYPE: &str = "Voxear.Interop.Bootstrap, Voxear.Core";

static STARTED: AtomicBool = AtomicBool::new(false);

type Initialize = unsafe extern "C" fn(*const NativeApi, *mut ManagedApi) -> i32;

pub struct CreatedBehaviour {
    pub registration: Registration,
    /// The type implements `IInstance`.
    pub is_instance: bool,
}

/// The C# side of the engine. CoreCLR cannot be restarted, so there is at most
/// one per process.
pub struct ScriptRuntime {
    managed: ManagedApi,
    // Scripts hold pointers into both of these, so they live on the heap.
    _native: Box<NativeApi>,
    time: Box<Time>,
}

impl ScriptRuntime {
    /// Starts .NET and initialises `Voxear.Core.dll`, whose
    /// `Voxear.Core.runtimeconfig.json` must sit next to it.
    pub fn start(install: &DotnetInstall, core_assembly: &Path) -> Result<Self> {
        ensure!(!STARTED.swap(true, Ordering::SeqCst), "the .NET runtime can only start once per process");
        let config = core_assembly.with_extension("runtimeconfig.json");
        ensure!(config.is_file(), "{} is missing; build managed/Voxear.slnx first", config.display());

        let runtime = Runtime::start(install, &config)?;
        runtime.load_assembly(core_assembly)?;
        let initialize = runtime.function(BOOTSTRAP_TYPE, "Initialize")?;
        let initialize: Initialize = unsafe { std::mem::transmute(initialize) };

        let time = Box::new(Time::default());
        let native = Box::new(interop::native_api(&*time));
        let mut managed = ManagedApi::default();
        let status = unsafe { initialize(&*native, &mut managed) };
        ensure!(status == 0, "Voxear.Core failed to initialise (status {status})");
        ensure!(
            managed.version == interop::API_VERSION && managed.size == size_of::<ManagedApi>() as u32,
            "Voxear.Core speaks interop v{} but the engine expects v{}; rebuild the managed projects",
            managed.version,
            interop::API_VERSION
        );
        ensure!(
            managed.load_game_assembly.is_some()
                && managed.create_behaviour.is_some()
                && managed.invoke.is_some()
                && managed.release.is_some(),
            "Voxear.Core left part of its interop table empty"
        );
        interop::set_create_behaviour(managed.create_behaviour.expect("validated above"));
        Ok(Self { managed, _native: native, time })
    }

    /// Loads game scripts into their own unloadable context.
    pub fn load_game_assembly(&mut self, path: &Path) -> Result<()> {
        let path = path.canonicalize().with_context(|| format!("{} not found", path.display()))?;
        let path = path.to_str().context("assembly path is not valid UTF-8")?;
        let load = self.managed.load_game_assembly.expect("validated at start");
        let status = unsafe { load(path.as_ptr(), path.len() as i32) };
        ensure!(status == 0, "could not load game scripts from {path}");
        Ok(())
    }

    /// Instantiates a behaviour type, e.g. `Hello.Spinner`, on an Obj. Its
    /// fields are set just before its Awake. The result still has to be
    /// registered with the lifecycle.
    pub fn create_behaviour(
        &mut self,
        world: &mut World,
        spec: &BehaviourSpec,
        entity: Entity,
    ) -> Result<CreatedBehaviour> {
        let create = self.managed.create_behaviour.expect("validated at start");
        let mut info = BehaviourInfo::default();
        let handle = {
            let _scope = WorldScope::enter(world);
            interop::call_create(create, spec, entity, &mut info)
        };
        let Some(handle) = handle else {
            bail!("could not create behaviour `{}`; is it a Behaviour in the game assembly?", spec.type_name);
        };
        Ok(CreatedBehaviour {
            registration: Registration {
                handle,
                entity,
                callbacks: Callbacks(info.callbacks),
                order: info.order,
                enabled: true,
            },
            is_instance: info.flags & FLAG_INSTANCE != 0,
        })
    }

    /// Objs scripts asked to destroy since the last call.
    pub fn take_obj_destroys(&mut self) -> Vec<Entity> {
        interop::take_obj_destroys()
    }

    /// Levels scripts asked to load since the last call, oldest first.
    pub fn take_level_requests(&mut self) -> Vec<String> {
        interop::take_level_requests()
    }

    pub fn time(&self) -> &Time {
        &self.time
    }

    pub fn time_mut(&mut self) -> &mut Time {
        &mut self.time
    }

    /// Pairs the runtime with the world so the lifecycle can drive scripts.
    pub fn host<'a>(&'a self, world: &'a mut World) -> ScriptHost<'a> {
        ScriptHost { runtime: self, world }
    }
}

pub struct ScriptHost<'a> {
    runtime: &'a ScriptRuntime,
    world: &'a mut World,
}

impl BehaviourHost for ScriptHost<'_> {
    fn invoke(&mut self, phase: Phase, behaviours: &[BehaviourHandle]) {
        let invoke = self.runtime.managed.invoke.expect("validated at start");
        let _scope = WorldScope::enter(self.world);
        unsafe { invoke(phase as i32, behaviours.as_ptr().cast(), behaviours.len() as i32) };
    }

    fn release(&mut self, behaviours: &[BehaviourHandle]) {
        let release = self.runtime.managed.release.expect("validated at start");
        let _scope = WorldScope::enter(self.world);
        unsafe { release(behaviours.as_ptr().cast(), behaviours.len() as i32) };
    }

    fn drain_requests(&mut self, out: &mut Vec<Request>) {
        interop::drain_requests(out);
    }
}
