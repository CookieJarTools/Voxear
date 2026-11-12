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
    physics::PhysicsEventData,
    saves::{self, SaveSettings},
    signals,
};

const BOOTSTRAP_TYPE: &str = "Voxear.Interop.Bootstrap, Voxear.Core";

static STARTED: AtomicBool = AtomicBool::new(false);

type Initialize = unsafe extern "C" fn(*const NativeApi, *mut ManagedApi) -> i32;

/// A command or variable a development build's console takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleName {
    pub name: String,
    /// How it is typed: `SpawnWave count [mode]`, or `God = False`.
    pub usage: String,
    /// What it is for.
    pub help: String,
}

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

/// The managed heap, as .NET counts it: the bytes in use now, those
/// committed, all ever allocated, and each generation's size after the last
/// collection: 0, 1 and 2, the large objects' and the pinned objects'.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ManagedMemory {
    pub heap: u64,
    pub committed: u64,
    pub allocated: u64,
    pub generations: [u64; 5],
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

        signals::prepare_thread();
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
                && managed.add_component.is_some()
                && managed.console_run.is_some()
                && managed.console_names.is_some()
                && managed.run_check.is_some()
                && managed.inspector_call.is_some()
                && managed.managed_memory.is_some()
                && managed.physics_events.is_some()
                && managed.motion_marks.is_some()
                && managed.take_net_fields.is_some()
                && managed.set_enabled.is_some()
                && managed.origin_moved.is_some()
                && managed.particle_hits.is_some(),
            "Voxear.Core left part of its interop table empty"
        );
        interop::set_create_behaviour(
            managed.create_behaviour.expect("validated above"),
            managed.add_component.expect("validated above"),
        );
        Ok(Self { managed, _native: native, time, loaded: None })
    }

    /// The C# entry points, for a call from this thread, which first gets a
    /// signal stack C#'s null dereferences can be caught on.
    pub(crate) fn api(&self) -> &ManagedApi {
        signals::prepare_thread();
        &self.managed
    }

    /// Loads game scripts into their own unloadable context.
    pub fn load_game_assembly(&mut self, path: &Path) -> Result<()> {
        let path = path.canonicalize().with_context(|| format!("{} not found", path.display()))?;
        let path = path.to_str().context("assembly path is not valid UTF-8")?;
        let load = self.api().load_game_assembly.expect("validated at start");
        let status = unsafe { load(path.as_ptr(), path.len() as i32) };
        ensure!(status == 0, "could not load game scripts from {path}");
        self.loaded = Some((PathBuf::from(path), built(Path::new(path))));
        Ok(())
    }

    /// Whether the game scripts were built again since they were loaded,
    /// or a package's assembly was: a change to a package's script builds
    /// only its own assembly, not the game's.
    pub fn is_out_of_date(&self) -> bool {
        let Some((path, loaded)) = &self.loaded else { return false };
        built(path).is_some_and(|built| loaded.is_none_or(|loaded| built > loaded))
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
        let built_at = built(&path);
        let reload = self.api().reload_game_assembly.expect("validated at start");
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
        self.loaded = Some((path, built_at));
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
        let Some(switch) = self.api().switch_level else { return };
        let _scope = WorldScope::enter(world);
        unsafe { switch(level.as_ptr(), level.len() as i32) };
    }

    /// Runs a `[Command]` method of the game scripts against `world`, as the
    /// editor's palette asks. Fails when the method is not there or throws;
    /// what it threw is logged.
    pub fn run_command(&mut self, world: &mut World, type_name: &str, method: &str) -> Result<()> {
        let run = self.api().run_command.context("Voxear.Core cannot run commands")?;
        let _scope = WorldScope::enter(world);
        let status = unsafe { run(type_name.as_ptr(), type_name.len() as i32, method.as_ptr(), method.len() as i32) };
        ensure!(status != 1, "{type_name} has no [Command] method {method}");
        ensure!(status == 0, "{type_name}.{method} threw; the Console has what it said");
        Ok(())
    }

    /// Runs a check a VoxDocs guide's step names, against `world`: a static
    /// method of the game scripts that takes nothing and returns `bool`, true
    /// when it passes, or `string`, empty when it passes and otherwise what is
    /// still wrong. `Ok(Err(why))` when it does not pass, `why` empty when it
    /// said nothing; an error when there is no such method or it threw, which
    /// is logged.
    pub fn run_check(&mut self, world: &mut World, type_name: &str, method: &str) -> Result<Result<(), String>> {
        let run = self.api().run_check.expect("validated at start");
        let _scope = WorldScope::enter(world);
        let mut buffer = vec![0u8; 1024];
        let mut written = 0i32;
        let (ty, name) = (type_name.as_bytes(), method.as_bytes());
        let status = unsafe {
            let (buffer_len, buffer_ptr) = (buffer.len() as i32, buffer.as_mut_ptr());
            run(ty.as_ptr(), ty.len() as i32, name.as_ptr(), name.len() as i32, buffer_ptr, buffer_len, &mut written)
        };
        ensure!(
            status != 2,
            "{type_name} has no check {method}: a static method taking nothing and returning bool or string"
        );
        ensure!(status != 3, "{type_name}.{method} threw; the Console has what it said");
        if status == 0 {
            return Ok(Ok(()));
        }
        buffer.truncate(written.clamp(0, buffer.len() as i32) as usize);
        Ok(Err(String::from_utf8_lossy(&buffer).into_owned()))
    }

    /// The managed heap as .NET counts it.
    pub fn managed_memory(&self) -> ManagedMemory {
        let read = self.api().managed_memory.expect("validated at start");
        let mut values = [0u64; 8];
        let written = unsafe { read(values.as_mut_ptr(), values.len() as i32) }.clamp(0, 8) as usize;
        let [heap, committed, allocated, generations @ ..] = values;
        let mut sizes = [0; 5];
        let kept = written.saturating_sub(3);
        sizes[..kept].copy_from_slice(&generations[..kept]);
        ManagedMemory { heap, committed, allocated, generations: sizes }
    }

    /// Runs a line typed in a development build's console: a `[Command]`
    /// method by name with its arguments, or a `[ConsoleVariable]`, read or
    /// set. What it did is logged. False when nothing has its first word as
    /// a name.
    pub fn run_console(&mut self, world: &mut World, line: &str) -> Result<bool> {
        let run = self.api().console_run.expect("validated at start");
        let _scope = WorldScope::enter(world);
        let status = unsafe { run(line.as_ptr(), line.len() as i32) };
        ensure!(status != 2, "`{line}` failed; the log has what went wrong");
        Ok(status == 0)
    }

    /// The console's commands and variables: each one's name, how it is
    /// typed, and what it is for.
    pub fn console_names(&self) -> Vec<ConsoleName> {
        let call = self.api().console_names.expect("validated at start");
        let mut buffer = vec![0u8; 4096];
        loop {
            let length = unsafe { call(buffer.as_mut_ptr(), buffer.len() as i32) };
            if length < 0 {
                return Vec::new();
            }
            if length as usize <= buffer.len() {
                buffer.truncate(length as usize);
                break;
            }
            buffer.resize(length as usize, 0);
        }
        String::from_utf8_lossy(&buffer)
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                let (name, usage, help) = (parts.next()?, parts.next()?, parts.next().unwrap_or(""));
                Some(ConsoleName { name: name.into(), usage: usage.into(), help: help.into() })
            })
            .collect()
    }

    /// Every `[ContentRoot]` pattern in the game scripts, with the type or
    /// member that declares it.
    pub fn content_roots(&self) -> Result<Vec<(String, String)>> {
        let call = self.api().content_roots.expect("validated at start");
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
        let create = self.api().create_behaviour.expect("validated at start");
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
                enabled: spec.enabled,
            },
            is_instance: info.flags & FLAG_INSTANCE != 0,
        })
    }

    /// Runs the C# systems (`SystemBase`) of the engine stage at `stage` in
    /// its stages, in their order; the entity commands they made wait until
    /// each one ends, and anything left waiting applies before this returns.
    pub fn run_systems(&mut self, world: &mut World, stage: usize) {
        let run = self.api().run_systems.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { run(stage as i32) };
        interop::systems_ran();
    }

    /// Gives an Obj a C# component (an `IComponent` struct, e.g.
    /// `Hello.Spin`) with the spec's field values, as a scene or prefab does.
    pub fn add_component(&mut self, world: &mut World, spec: &BehaviourSpec, entity: Entity) -> Result<()> {
        let add = self.api().add_component.expect("validated at start");
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
        let update = self.api().update.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { update() };
    }

    /// Tells behaviours of a physics step's contacts and trigger
    /// crossings, as `OnCollisionEnter`, `OnTriggerExit` and the rest,
    /// each Obj in one told of it.
    pub fn physics_events(&mut self, world: &mut World, events: &[PhysicsEventData]) {
        if events.is_empty() {
            return;
        }
        let send = self.api().physics_events.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { send(events.as_ptr(), events.len() as i32) };
    }

    /// Tells scripts of particles' strikes, as `OnParticleHit` on the
    /// behaviours of each system's Obj and `Particles.Hit`.
    pub fn particle_hits(&mut self, world: &mut World, hits: &[crate::physics::ParticleHitData]) {
        if hits.is_empty() {
            return;
        }
        let send = self.api().particle_hits.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { send(hits.as_ptr(), hits.len() as i32) };
    }

    /// Tells behaviours of the marks actions passed, as `OnMotionMark`:
    /// each Obj told of its marks.
    pub fn motion_marks(&mut self, world: &mut World, marks: &[(Entity, String)]) {
        if marks.is_empty() {
            return;
        }
        let send = self.api().motion_marks.expect("validated at start");
        let entities: Vec<u64> = marks.iter().map(|(entity, _)| entity.to_bits()).collect();
        let text = marks.iter().map(|(_, mark)| mark.as_str()).collect::<Vec<_>>().join("\n");
        let _scope = WorldScope::enter(world);
        unsafe { send(entities.as_ptr(), entities.len() as i32, text.as_ptr(), text.len() as i32) };
    }

    /// Tells scripts the world's origin moved by `by` whole metres: their
    /// `[WorldPosition]` fields move with every Obj, then `Origin.Moved` is
    /// raised.
    pub fn origin_moved(&mut self, world: &mut World, by: [i32; 3]) {
        let send = self.api().origin_moved.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { send(by.as_ptr()) };
    }

    /// Enables or disables a behaviour as its script setting `Enabled`
    /// would: what it asks for waits in the requests the lifecycle takes.
    pub fn set_behaviour_enabled(&mut self, world: &mut World, handle: BehaviourHandle, enabled: bool) {
        let set = self.api().set_enabled.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { set(handle.0, i32::from(enabled)) };
    }

    /// Sets the replicated properties that came from the server on their
    /// behaviours now.
    pub fn take_net_fields(&mut self, world: &mut World) {
        let take = self.api().take_net_fields.expect("validated at start");
        let _scope = WorldScope::enter(world);
        unsafe { take() };
    }

    /// Objs scripts asked to destroy since the last call.
    pub fn take_obj_destroys(&mut self) -> Vec<Entity> {
        interop::take_obj_destroys()
    }

    /// Levels scripts asked to load since the last call, oldest first.
    pub fn take_level_requests(&mut self) -> Vec<String> {
        interop::take_level_requests()
    }

    /// Lighting settings scripts asked to light the level with since the
    /// last call, oldest first.
    pub fn take_lighting_requests(&mut self) -> Vec<String> {
        interop::take_lighting_requests()
    }

    /// The look packs scripts asked to wear since the last call, oldest
    /// first: what names each, an empty one for none, and whether the
    /// choice is to be kept for the game's next run.
    pub fn take_look_pack_requests(&mut self) -> Vec<(String, bool)> {
        interop::take_look_pack_requests()
    }

    /// The texture packs scripts asked to wear since the last call, oldest
    /// first: what names each, an empty one for none, and whether the
    /// choice is to be kept for the game's next run.
    pub fn take_texture_pack_requests(&mut self) -> Vec<(String, bool)> {
        interop::take_texture_pack_requests()
    }

    /// Tells scripts the level's weather.
    pub fn publish_weather(&mut self, state: interop::WeatherState) {
        interop::publish_weather(state);
    }

    /// What scripts asked of the weather since the last call, if they did: the last of it.
    pub fn take_weather(&mut self) -> Option<interop::WeatherRequest> {
        interop::take_weather()
    }

    /// Tells scripts the level's time of year, or that it has no Seasons.
    pub fn publish_time_of_year(&mut self, time: Option<f32>) {
        interop::publish_time_of_year(time);
    }

    /// The time of year scripts set since the last call, if they did.
    pub fn take_time_of_year(&mut self) -> Option<f32> {
        interop::take_time_of_year()
    }

    /// Tells scripts which of the sea's costlier looks are on, or that the
    /// level has no Ocean.
    pub fn publish_water_features(&mut self, features: Option<u32>) {
        interop::publish_water_features(features);
    }

    /// The sea's looks scripts turned on and off since the last call, if they
    /// did.
    pub fn take_water_features(&mut self) -> Option<u32> {
        interop::take_water_features()
    }

    /// What scripts asked of loaded scenes since the last call, oldest first.
    pub fn take_scene_requests(&mut self) -> Vec<crate::SceneRequest> {
        interop::take_scene_requests()
    }

    pub fn time(&self) -> &Time {
        &self.time
    }

    pub fn time_mut(&mut self) -> &mut Time {
        &mut self.time
    }

    /// Where `Voxear.Save` keeps the game's saves, and the format it writes
    /// when scripts choose none. Until this is set, saving fails with a
    /// message that there is nowhere to keep saves.
    pub fn set_saves(&mut self, settings: SaveSettings) {
        saves::configure(settings);
    }

    /// Pairs the runtime with the world so the lifecycle can drive scripts.
    pub fn host<'a>(&'a self, world: &'a mut World) -> ScriptHost<'a> {
        ScriptHost { runtime: self, world }
    }
}

impl Drop for ScriptRuntime {
    /// Saves still being written finish first, so quitting right after a
    /// save keeps it.
    fn drop(&mut self) {
        saves::flush();
    }
}

pub struct ScriptHost<'a> {
    runtime: &'a ScriptRuntime,
    world: &'a mut World,
}

impl BehaviourHost for ScriptHost<'_> {
    fn invoke(&mut self, phase: Phase, behaviours: &[BehaviourHandle]) {
        let invoke = self.runtime.api().invoke.expect("validated at start");
        let _scope = WorldScope::enter(self.world);
        unsafe { invoke(phase as i32, behaviours.as_ptr().cast(), behaviours.len() as i32) };
    }

    fn release(&mut self, behaviours: &[BehaviourHandle]) {
        let release = self.runtime.api().release.expect("validated at start");
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

/// The file a build of the scripts writes beside the game's assembly to
/// name the assemblies of the project's packages, one file name a line,
/// each after the ones it uses. `Voxear.Core` loads them with the game's,
/// into the same context, and a player build ships it with them.
pub const PACKAGE_ASSEMBLIES: &str = "Packages.txt";

/// The packages' assemblies the game's assembly at `game` was built with,
/// from the list beside it: none for a project without packages.
pub fn package_assemblies(game: &Path) -> Vec<PathBuf> {
    let Some(folder) = game.parent() else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(folder.join(PACKAGE_ASSEMBLIES)) else { return Vec::new() };
    text.lines().map(str::trim).filter(|line| !line.is_empty()).map(|line| folder.join(line)).collect()
}

/// When the scripts at `game` were last built: the newest of the game's
/// assembly and its packages'.
fn built(game: &Path) -> Option<SystemTime> {
    let packages = package_assemblies(game).into_iter().filter_map(|path| modified(&path));
    modified(game).into_iter().chain(packages).max()
}
