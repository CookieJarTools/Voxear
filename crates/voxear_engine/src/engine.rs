use anyhow::Result;
use voxear_core::time::Time;
use voxear_ecs::{Entity, Schedule, World};
use voxear_scene::{
    BehaviourHandle, BehaviourHost, Behaviours, InstanceMarker, Lifecycle, Name, Phase, Transform,
    propagate_transforms, register_obj_components,
};
use voxear_scripting::ScriptRuntime;

/// Engine stages in run order. Behaviour callbacks run alongside them: see
/// [`Engine::tick`].
pub const STAGES: &[&str] = &["First", "FixedUpdate", "Update", "PostUpdate", "Last"];

/// Fixed steps allowed per frame, so a slow frame cannot snowball into
/// ever more catch-up work.
const MAX_FIXED_STEPS: u32 = 5;

/// The simulation: world, systems, behaviours and time. Rendering and
/// windowing sit outside so the same engine runs headless, in the player and
/// in the editor.
pub struct Engine {
    pub world: World,
    pub schedule: Schedule,
    lifecycle: Lifecycle,
    scripts: Option<ScriptRuntime>,
    time: Time,
    fixed_accumulator: f32,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        let mut world = World::new();
        register_obj_components(&mut world);
        let mut schedule = Schedule::new(STAGES);
        schedule.add_system("PostUpdate", "propagate transforms", propagate_transforms);
        Self {
            world,
            schedule,
            lifecycle: Lifecycle::new(),
            scripts: None,
            time: Time::default(),
            fixed_accumulator: 0.0,
        }
    }

    pub fn set_scripts(&mut self, scripts: ScriptRuntime) {
        self.scripts = Some(scripts);
    }

    pub fn scripts_mut(&mut self) -> Option<&mut ScriptRuntime> {
        self.scripts.as_mut()
    }

    pub fn time(&self) -> &Time {
        &self.time
    }

    /// Creates an Obj and attaches behaviours by C# type name. Behaviours get
    /// their first callbacks at the next tick, as one batch.
    pub fn spawn_obj(&mut self, name: &str, transform: Transform, behaviours: &[String]) -> Result<Entity> {
        let entity = self.world.spawn((Name(name.to_string()), transform));
        if behaviours.is_empty() {
            return Ok(entity);
        }
        let Some(scripts) = self.scripts.as_mut() else {
            anyhow::bail!("{name} has behaviours but the project has no scripts");
        };

        let mut handles = Vec::with_capacity(behaviours.len());
        let mut is_instance = false;
        for type_name in behaviours {
            let created = scripts.create_behaviour(&mut self.world, type_name, entity)?;
            is_instance |= created.is_instance;
            handles.push(created.registration.handle);
            self.lifecycle.register(created.registration);
        }
        self.world.insert(entity, Behaviours(handles));
        if is_instance {
            self.world.insert(entity, InstanceMarker);
        }
        Ok(entity)
    }

    /// Advances one frame:
    ///
    /// 1. `First` systems, then pending behaviours get Awake/OnEnable/Start.
    /// 2. Fixed steps: FixedUpdate behaviours, then `FixedUpdate` systems.
    /// 3. Update behaviours, then `Update` systems.
    /// 4. LateUpdate behaviours, then `PostUpdate` systems.
    /// 5. Deferred destroys, then `Last` systems.
    ///
    /// Anything created or enabled along the way is initialised before the
    /// next phase.
    pub fn tick(&mut self, unscaled_delta: f32) {
        self.time.advance(unscaled_delta);
        self.sync_script_time(self.time.delta);

        self.schedule.run_stage("First", &mut self.world);
        self.flush();

        self.fixed_accumulator += self.time.delta;
        let mut steps = 0;
        while self.fixed_accumulator >= self.time.fixed_delta && steps < MAX_FIXED_STEPS {
            self.sync_script_time(self.time.fixed_delta);
            self.run_phase(Phase::FixedUpdate);
            self.schedule.run_stage("FixedUpdate", &mut self.world);
            self.flush();
            self.fixed_accumulator -= self.time.fixed_delta;
            steps += 1;
        }
        if steps == MAX_FIXED_STEPS {
            self.fixed_accumulator = 0.0;
        }
        self.sync_script_time(self.time.delta);

        self.run_phase(Phase::Update);
        self.schedule.run_stage("Update", &mut self.world);
        self.flush();

        self.run_phase(Phase::LateUpdate);
        self.schedule.run_stage("PostUpdate", &mut self.world);
        self.flush();

        self.with_host(|lifecycle, host| lifecycle.end_of_frame(host));
        self.schedule.run_stage("Last", &mut self.world);
    }

    fn flush(&mut self) {
        self.with_host(|lifecycle, host| lifecycle.flush(host));
    }

    fn run_phase(&mut self, phase: Phase) {
        self.with_host(|lifecycle, host| lifecycle.run(phase, host));
    }

    /// Scripts see `Time.Delta` as the fixed step inside FixedUpdate, as in Unity.
    fn sync_script_time(&mut self, delta: f32) {
        if let Some(scripts) = self.scripts.as_mut() {
            *scripts.time_mut() = Time { delta, ..self.time };
        }
    }

    fn with_host(&mut self, f: impl FnOnce(&mut Lifecycle, &mut dyn BehaviourHost)) {
        match &self.scripts {
            Some(scripts) => f(&mut self.lifecycle, &mut scripts.host(&mut self.world)),
            None => f(&mut self.lifecycle, &mut NoScripts),
        }
    }
}

/// Stands in when a project has no scripts, so the lifecycle code has one path.
struct NoScripts;

impl BehaviourHost for NoScripts {
    fn invoke(&mut self, _: Phase, _: &[BehaviourHandle]) {}

    fn release(&mut self, _: &[BehaviourHandle]) {}
}
