use std::borrow::Cow;

use crate::world::World;

pub trait System: Send + 'static {
    fn name(&self) -> &str;
    fn run(&mut self, world: &mut World);
}

struct FnSystem<F> {
    name: Cow<'static, str>,
    run: F,
}

impl<F: FnMut(&mut World) + Send + 'static> System for FnSystem<F> {
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World) {
        (self.run)(world);
    }
}

struct Stage {
    label: &'static str,
    systems: Vec<Box<dyn System>>,
}

/// What runs around each system, given its name, as a profiler times them:
/// the first before it, the second after.
pub type SystemTimer = (fn(&str), fn());

/// Systems grouped into ordered stages. Stages always run in the order they
/// were declared and systems within a stage in the order they were added, so
/// execution order is deterministic.
pub struct Schedule {
    stages: Vec<Stage>,
    timer: Option<SystemTimer>,
}

impl Schedule {
    pub fn new(stages: &[&'static str]) -> Self {
        Self { stages: stages.iter().map(|&label| Stage { label, systems: Vec::new() }).collect(), timer: None }
    }

    /// Runs `timer` around every system from now on, or nothing with `None`.
    pub fn set_system_timer(&mut self, timer: Option<SystemTimer>) {
        self.timer = timer;
    }

    pub fn add_system(
        &mut self,
        stage: &str,
        name: impl Into<Cow<'static, str>>,
        run: impl FnMut(&mut World) + Send + 'static,
    ) -> &mut Self {
        self.add_boxed(stage, Box::new(FnSystem { name: name.into(), run }))
    }

    pub fn add_boxed(&mut self, stage: &str, system: Box<dyn System>) -> &mut Self {
        let Some(stage) = self.stages.iter_mut().find(|s| s.label == stage) else {
            panic!("unknown stage `{stage}`");
        };
        stage.systems.push(system);
        self
    }

    pub fn run(&mut self, world: &mut World) {
        for stage in &mut self.stages {
            run_systems(&mut stage.systems, world, self.timer);
        }
    }

    pub fn run_stage(&mut self, stage: &str, world: &mut World) {
        if let Some(stage) = self.stages.iter_mut().find(|s| s.label == stage) {
            run_systems(&mut stage.systems, world, self.timer);
        }
    }

    pub fn stages(&self) -> impl Iterator<Item = (&'static str, impl Iterator<Item = &str>)> {
        self.stages.iter().map(|s| (s.label, s.systems.iter().map(|sys| sys.name())))
    }
}

fn run_systems(systems: &mut [Box<dyn System>], world: &mut World, timer: Option<SystemTimer>) {
    for system in systems {
        if let Some((begin, _)) = timer {
            begin(system.name());
        }
        system.run(world);
        if let Some((_, end)) = timer {
            end();
        }
        world.advance_tick();
    }
}
