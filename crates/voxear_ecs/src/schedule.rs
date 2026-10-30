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

/// Systems grouped into ordered stages. Stages always run in the order they
/// were declared and systems within a stage in the order they were added, so
/// execution order is deterministic.
pub struct Schedule {
    stages: Vec<Stage>,
}

impl Schedule {
    pub fn new(stages: &[&'static str]) -> Self {
        Self { stages: stages.iter().map(|&label| Stage { label, systems: Vec::new() }).collect() }
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
            run_systems(&mut stage.systems, world);
        }
    }

    pub fn run_stage(&mut self, stage: &str, world: &mut World) {
        if let Some(stage) = self.stages.iter_mut().find(|s| s.label == stage) {
            run_systems(&mut stage.systems, world);
        }
    }

    pub fn stages(&self) -> impl Iterator<Item = (&'static str, impl Iterator<Item = &str>)> {
        self.stages.iter().map(|s| (s.label, s.systems.iter().map(|sys| sys.name())))
    }
}

fn run_systems(systems: &mut [Box<dyn System>], world: &mut World) {
    for system in systems {
        system.run(world);
        world.advance_tick();
    }
}
