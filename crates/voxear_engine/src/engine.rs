use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail};
use voxear_core::time::Time;
use voxear_ecs::{Entity, Schedule, World};
use voxear_scene::{
    AttachedBehaviour, BehaviourHandle, BehaviourHost, BehaviourSpec, Behaviours, Destroying, InstanceMarker,
    Lifecycle, Name, Parent, Phase, Placement, Transform, propagate_transforms, register_obj_components,
};
use voxear_scripting::ScriptRuntime;

use crate::prefab::{FromPrefab, PrefabInstance, Template};

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
        world.register_named_component::<PrefabInstance>("Voxear.PrefabInstance");
        world.register_named_component::<FromPrefab>("Voxear.FromPrefab");
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

    pub fn scripts(&self) -> Option<&ScriptRuntime> {
        self.scripts.as_ref()
    }

    pub fn scripts_mut(&mut self) -> Option<&mut ScriptRuntime> {
        self.scripts.as_mut()
    }

    pub fn time(&self) -> &Time {
        &self.time
    }

    /// Creates an Obj and attaches behaviours by C# type name. Behaviours get
    /// their first callbacks at the next flush, as one batch.
    pub fn spawn_obj(&mut self, name: &str, transform: Transform, behaviours: &[String]) -> Result<Entity> {
        let entity = self.world.spawn((Name(name.to_string()), transform));
        let specs: Vec<BehaviourSpec> = behaviours.iter().map(|t| BehaviourSpec::new(t)).collect();
        self.attach_behaviours(entity, &specs)?;
        Ok(entity)
    }

    /// Spawns a template's Objs and attaches their behaviours, which start
    /// with the next batch, as a scene load does. Roots go under `parent`,
    /// and take `placement`, relative to it, when it is given.
    pub fn spawn_template(
        &mut self,
        template: &Template,
        parent: Option<Entity>,
        placement: Option<Placement>,
    ) -> Result<Vec<Entity>> {
        let entities = template.spawn_objs(&mut self.world, parent, placement);
        for (index, (obj, &entity)) in template.objs.iter().zip(&entities).enumerate() {
            let behaviours = template.behaviours(index, &entities);
            self.attach_behaviours(entity, &behaviours).with_context(|| format!("Obj {}", obj.name))?;
        }
        Ok(entities)
    }

    /// Spawns a prefab like Unity's `Instantiate`: every new behaviour gets
    /// Awake, then OnEnable, before this returns, and Start at the next
    /// flush. Returns the root.
    pub fn instantiate(
        &mut self,
        template: &Template,
        parent: Option<Entity>,
        placement: Option<Placement>,
    ) -> Result<Entity> {
        let root = template.root().context("the prefab has no Objs")?;
        let entities = self.spawn_template(template, parent, placement)?;
        let handles: Vec<BehaviourHandle> =
            entities.iter().filter_map(|e| self.world.get::<Behaviours>(*e)).flat_map(Behaviours::handles).collect();
        self.with_host(|lifecycle, host| lifecycle.activate(&handles, host));
        Ok(entities[root])
    }

    /// Attaches a behaviour by C# type name and runs its Awake and OnEnable
    /// before returning, like Unity's `AddComponent`.
    pub fn add_behaviour(&mut self, entity: Entity, type_name: &str) -> Result<BehaviourHandle> {
        let handles = self.attach_behaviours(entity, &[BehaviourSpec::new(type_name)])?;
        self.with_host(|lifecycle, host| lifecycle.activate(&handles, host));
        Ok(handles[0])
    }

    /// Creates behaviours on an Obj and queues them for the next batch.
    fn attach_behaviours(&mut self, entity: Entity, specs: &[BehaviourSpec]) -> Result<Vec<BehaviourHandle>> {
        if specs.is_empty() {
            return Ok(Vec::new());
        }
        let Some(scripts) = self.scripts.as_mut() else {
            let name = self.world.get::<Name>(entity).map_or("An Obj", |n| n.0.as_str());
            bail!("{name} has behaviours but the project has no scripts");
        };

        let mut attached = Vec::with_capacity(specs.len());
        let mut is_instance = false;
        for spec in specs {
            let created = scripts.create_behaviour(&mut self.world, spec, entity)?;
            is_instance |= created.is_instance;
            attached.push(AttachedBehaviour { handle: created.registration.handle, spec: spec.clone() });
            self.lifecycle.register(created.registration);
        }
        let handles = attached.iter().map(|b| b.handle).collect();
        match self.world.get_mut::<Behaviours>(entity) {
            Some(behaviours) => behaviours.0.extend(attached),
            None => {
                self.world.insert(entity, Behaviours(attached));
            }
        }
        if is_instance {
            self.world.insert(entity, InstanceMarker);
        }
        Ok(handles)
    }

    /// Destroys Objs and all their children at the end of the frame, after
    /// their behaviours' OnDisable and OnDestroy, like Unity's `Destroy`.
    /// They stop rendering immediately.
    pub fn destroy_objs(&mut self, roots: &[Entity]) {
        let mut children: HashMap<Entity, Vec<Entity>> = HashMap::new();
        for (child, parent) in self.world.query_ref::<(Entity, &Parent)>() {
            children.entry(parent.0).or_default().push(child);
        }
        let mut doomed = HashSet::new();
        let mut stack = roots.to_vec();
        while let Some(entity) = stack.pop() {
            if doomed.insert(entity) {
                stack.extend(children.get(&entity).into_iter().flatten());
            }
        }
        for entity in doomed {
            if !self.world.is_alive(entity) {
                continue;
            }
            if let Some(behaviours) = self.world.get::<Behaviours>(entity) {
                for handle in behaviours.handles().collect::<Vec<_>>() {
                    self.lifecycle.destroy(handle);
                }
            }
            self.world.insert(entity, Destroying);
        }
    }

    /// Advances one frame:
    ///
    /// 0. Scripts resume awaits that finished and run posted continuations.
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
        if let Some(scripts) = self.scripts.as_mut() {
            scripts.update(&mut self.world);
            self.with_host(|lifecycle, host| lifecycle.apply_requests(host));
        }

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

        // Objs destroyed by OnDestroy callbacks below get theirs next frame,
        // so only these are removed now.
        let destroyed: Vec<Entity> = self.world.query_ref::<Entity>().with::<Destroying>().into_iter().collect();
        self.with_host(|lifecycle, host| lifecycle.end_of_frame(host));
        for entity in destroyed {
            self.world.despawn(entity);
        }
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

    /// Runs lifecycle work with scripts attached, then destroys the Objs
    /// scripts asked to destroy meanwhile.
    fn with_host(&mut self, f: impl FnOnce(&mut Lifecycle, &mut dyn BehaviourHost)) {
        match &self.scripts {
            Some(scripts) => f(&mut self.lifecycle, &mut scripts.host(&mut self.world)),
            None => f(&mut self.lifecycle, &mut NoScripts),
        }
        let doomed = self.scripts.as_mut().map(ScriptRuntime::take_obj_destroys).unwrap_or_default();
        if !doomed.is_empty() {
            self.destroy_objs(&doomed);
        }
    }
}

/// Stands in when a project has no scripts, so the lifecycle code has one path.
struct NoScripts;

impl BehaviourHost for NoScripts {
    fn invoke(&mut self, _: Phase, _: &[BehaviourHandle]) {}

    fn release(&mut self, _: &[BehaviourHandle]) {}
}
