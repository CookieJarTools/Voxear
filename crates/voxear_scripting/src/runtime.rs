use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};

use anyhow::{Context, Result, bail, ensure};
use voxear_core::time::Time;
use voxear_ecs::{Entity, World};
use voxear_scene::{BehaviourHandle, BehaviourHost, BehaviourSpec, Callbacks, Phase, Registration, Request};

use crate::{
    hostfxr::{DotnetInstall, Runtime},
    interop::{self, BehaviourInfo, FLAG_GONE, FLAG_INSTANCE, ManagedApi, NativeApi, WorldScope},
};

const BOOTSTRAP_TYPE: &str = "Voxear.Interop.Bootstrap, Voxear.Core";

static STARTED: AtomicBool = AtomicBool::new(false);

type Initialize = unsafe extern "C" fn(*const NativeApi, *mut ManagedApi) -> i32;

pub struct CreatedBehaviour {
    pub registration: Registration,
    /// The type implements `IInstance`.
    pub is_instance: bool,
}

/// What became of a behaviour when the scripts reloaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reloaded {
    /// Made again from its class in the new build, which may call other
    /// callbacks or have another execution order now.
    Kept { callbacks: Callbacks, order: i32, is_instance: bool },
    /// Its class is not in the new build, so it was dropped.
    Gone,
}

/// The C# side of the engine. CoreCLR cannot be restarted, so there is at most
/// one per process.
pub struct ScriptRuntime {
    managed: ManagedApi,
    // Scripts hold pointers into both of these, so they live on the heap.
    _native: Box<NativeApi>,
    time: Box<Time>,
    /// The game scripts' assembly, and when the build loaded was written.
    loaded: Option<(PathBuf, Option<SystemTime>)>,
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
                && managed.release.is_some()
                && managed.update.is_some()
                && managed.content_roots.is_some()
                && managed.reload_game_assembly.is_some()
                && managed.run_systems.is_some()
                && managed.add_component.is_some(),
            "Voxear.Core left part of its interop table empty"
        );
        interop::set_create_behaviour(
            managed.create_behaviour.expect("validated above"),
            managed.add_component.expect("validated above"),
        );
        Ok(Self { managed, _native: native, time, loaded: None })
    }

    /// Loads game scripts into their own unloadable context.
    pub fn load_game_assembly(&mut self, path: &Path) -> Result<()> {
        let path = path.canonicalize().with_context(|| format!("{} not found", path.display()))?;
        let path = path.to_str().context("assembly path is not valid UTF-8")?;
        let load = self.managed.load_game_assembly.expect("validated at start");
        let status = unsafe { load(path.as_ptr(), path.len() as i32) };
        ensure!(status == 0, "could not load game scripts from {path}");
        self.loaded = Some((PathBuf::from(path), modified(Path::new(path))));
        Ok(())
    }

    /// Whether the game scripts were built again since they were loaded.
    pub fn is_out_of_date(&self) -> bool {
        let Some((path, loaded)) = &self.loaded else { return false };
        modified(path).is_some_and(|built| loaded.is_none_or(|loaded| built > loaded))
    }

    /// Loads the game scripts' new build in place of the running one, as
    /// hot reload does, keeping the game going: `Voxear.Core` gives every
    /// enabled behaviour OnDisable, makes each again from its class in the
    /// new build under the same handle with its fields carried over, and
    /// gives the enabled ones OnEnable (see `GameScripts.Reload`).
    /// `behaviours` are the behaviours the engine schedules, each with
    /// whether it is enabled; what became of each comes back in the same
    /// order. Fails, with nothing changed, when the build cannot be loaded.
    pub fn reload_game_assembly(
        &mut self,
        world: &mut World,
        behaviours: &[(BehaviourHandle, bool)],
    ) -> Result<Vec<Reloaded>> {
        let (path, _) = self.loaded.as_ref().context("the game scripts were never loaded")?;
        let path = path.clone();
        let text = path.to_str().context("assembly path is not valid UTF-8")?;
        let built = modified(&path);
        let reload = self.managed.reload_game_assembly.expect("validated at start");
        let handles: Vec<u32> = behaviours.iter().map(|(handle, _)| handle.0).collect();
        let enabled: Vec<u32> = behaviours.iter().map(|&(_, enabled)| u32::from(enabled)).collect();
        let mut infos: Vec<BehaviourInfo> = behaviours.iter().map(|_| BehaviourInfo::default()).collect();
        let status = {
            let _scope = WorldScope::enter(world);
            let count = handles.len() as i32;
            unsafe {
                reload(text.as_ptr(), text.len() as i32, handles.as_ptr(), enabled.as_ptr(), infos.as_mut_ptr(), count)
            }
        };
        ensure!(status != 1, "the new build of the scripts could not be loaded; the old one keeps running");
        ensure!(status == 0, "reloading the scripts failed partway; the Console has what went wrong");
        self.loaded = Some((path, built));
        Ok(infos
            .into_iter()
            .map(|info| match info.flags & FLAG_GONE {
                0 => Reloaded::Kept {
                    callbacks: Callbacks(info.callbacks),
                    order: info.order,
                    is_instance: info.flags & FLAG_INSTANCE != 0,
                },
                _ => Reloaded::Gone,
            })
            .collect())
    }

    /// Starts a level change through `Level.LoadAsync`, so its events fire
    /// as for a change scripts start, as the editor's Levels panel does
    /// while playing. Needs the engine's services lent, as while ticking.
    pub fn switch_level(&mut self, world: &mut World, level: &str) {
        let Some(switch) = self.managed.switch_level else { return };
        let _scope = WorldScope::enter(world);
        unsafe { switch(level.as_ptr(), level.len() as i32) };
    }

    /// Runs a `[Command]` method of the game scripts against `world`, as the
    /// editor's palette asks. Fails when the method is not there or throws;
    /// what it threw is logged.
    pub fn run_command(&mut self, world: &mut World, type_name: &str, method: &str) -> Result<()> {
        let run = self.managed.run_command.context("Voxear.Core cannot run commands")?;
        let _scope = WorldScope::enter(world);
        let status = unsafe { run(type_name.as_ptr(), type_name.len() as i32, method.as_ptr(), method.len() as i32) };
        ensure!(status != 1, "{type_name} has no [Command] method {method}");
        ensure!(status == 0, "{type_name}.{method} threw; the Console has what it said");
        Ok(())
    }

    /// Every `[ContentRoot]` pattern in the game scripts, with the type or
    /// member that declares it.
    pub fn content_roots(&self) -> Result<Vec<(String, String)>> {
        let call = self.managed.content_roots.expect("validated at start");
        let mut buffer = vec![0u8; 4096];
        loop {
            let length = unsafe { call(buffer.as_mut_ptr(), buffer.len() as i32) };
            ensure!(length >= 0, "could not read the scripts' [ContentRoot] attributes");
            let length = length as usize;
            if length <= buffer.len() {
                buffer.truncate(length);
                break;
            }
            buffer.resize(length, 0);
        }
        let text = String::from_utf8(buffer).context("content roots are not UTF-8")?;
        Ok(text.lines().filter_map(|line| line.split_once('\t')).map(|(p, s)| (p.into(), s.into())).collect())
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

    /// Runs the C# systems (`SystemBase`) of the engine stage at `stage` in
    /// its stages, in their order; the entity commands they made wait until
    /// each one ends, and anything left waiting applies before this returns.
    pub fn run_systems(&mut self, world: &mut World, stage: usize) {
        let run = self.managed.run_systems.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { run(stage as i32) };
        interop::systems_ran();
    }

    /// Gives an Obj a C# component (an `IComponent` struct, e.g.
    /// `Hello.Spin`) with the spec's field values, as a scene or prefab does.
    pub fn add_component(&mut self, world: &mut World, spec: &BehaviourSpec, entity: Entity) -> Result<()> {
        let add = self.managed.add_component.expect("validated at start");
        let added = {
            let _scope = WorldScope::enter(world);
            interop::call_with_fields(spec, |name, len, fields, fields_len| unsafe {
                add(name, len, entity.to_bits(), fields, fields_len)
            })
        };
        ensure!(
            added == 0,
            "could not add component `{}`; is it an IComponent struct in the game assembly?",
            spec.type_name
        );
        Ok(())
    }

    /// The once-per-frame script step: finished loads resume their awaits,
    /// and continuations posted to the main thread run.
    pub fn update(&mut self, world: &mut World) {
        let update = self.managed.update.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { update() };
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

/// When a file was last written.
fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|metadata| metadata.modified()).ok()
}
