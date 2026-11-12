//! Playing Particle Systems: each starts as it appears, its time counted
//! from then (`voxear_render::ParticlePlayback`), and scripts play and stop
//! it. For C#, `ParticleSystem` in Voxear.Core reads and writes an Obj's
//! system through one call, by number, as `lights` does a Light's.

use voxear_ecs::{Entity, World};
use voxear_render::{
    Blast, EmitShape, ForceField, ForceKind, ParticleBlasts, ParticleBlend, ParticleCollision, ParticlePlayback,
    ParticleSpace, ParticleSystem,
};

use crate::content::{linear, srgb};

// What `values` reads or writes, as `NativeParticles` in Voxear.Core numbers them.
const HAS: i32 = 0;
const ADD: i32 = 1;
const REMOVE: i32 = 2;
const PLAY: i32 = 3;
const STOP: i32 = 4;
const PLAYING: i32 = 5;
const DURATION: i32 = 6;
const LOOPING: i32 = 7;
const RATE: i32 = 8;
const BURST: i32 = 9;
const LIFETIME: i32 = 10;
const SPEED: i32 = 11;
const SIZE: i32 = 12;
const SIZE_END: i32 = 13;
const COLOR: i32 = 14;
const COLOR_END: i32 = 15;
const INTENSITY: i32 = 16;
const SHAPE: i32 = 17;
const RADIUS: i32 = 18;
const ANGLE: i32 = 19;
const BOX_SIZE: i32 = 20;
const GRAVITY: i32 = 21;
const DRAG: i32 = 22;
const SPIN: i32 = 23;
const BLEND: i32 = 24;
const LIT: i32 = 25;
const SOFTNESS: i32 = 26;
const SEED: i32 = 27;
const SPACE: i32 = 28;
const WIND: i32 = 29;
const TURBULENCE: i32 = 30;
const TURBULENCE_SIZE: i32 = 31;
const SIMULATED: i32 = 32;
const COLLISION: i32 = 33;
const BOUNCE: i32 = 34;
const FRICTION: i32 = 35;
const LIFE_LOST: i32 = 36;
const FORCES: i32 = 37;
const STRETCH: i32 = 38;
const DELAY: i32 = 39;

/// The engine's time, as Particle Systems are played and stopped by.
#[derive(Clone, Copy, Debug, Default)]
struct ParticleClock(f64);

/// Sets what answers `ParticleSystem`, `ForceField` and `Particles.Push` in
/// scripts.
pub(crate) fn register() {
    voxear_scripting::set_particle_values(values);
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::ForceField, field_values);
    voxear_scripting::set_particle_blaster(|world, at, radius, speed| blast(world, at.into(), radius, speed));
}

/// Reads what the world's Particle Systems name, for the renderer to find
/// (see `voxear_render::effects`): the effects they play, from the
/// project's `.particles` assets, and the pictures they and those effects'
/// emitters draw with, which need `renderer` to load. Each is read once and
/// kept by `content`, so this is a few lookups a system each frame, and an
/// effect whose file changed is taken up as soon as `content` forgets it.
pub fn resolve(world: &mut World, content: &mut crate::content::Content, renderer: Option<&mut voxear_render::Renderer>) {
    use voxear_render::{EffectRef, ParticleEffects, ParticlePictures, ParticleTexture};
    let mut effects = ParticleEffects::default();
    let mut wanted = std::collections::HashSet::new();
    for (system,) in world.query_ref::<(&ParticleSystem,)>() {
        let emitters: Vec<ParticleTexture> = match system.effect {
            None => vec![system.texture],
            Some(EffectRef::Builtin(_)) => Vec::new(),
            Some(EffectRef::Asset(guid)) => {
                let found = effects.0.get(&guid).cloned().or_else(|| content.particle_effect(guid));
                let Some(effect) = found else { continue };
                let textures = effect.emitters.iter().map(|emitter| emitter.system.texture).collect();
                effects.0.insert(guid, effect);
                textures
            }
        };
        wanted.extend(emitters.into_iter().filter_map(|texture| match texture {
            ParticleTexture::Asset(guid) => Some(guid),
            ParticleTexture::Builtin(_) => None,
        }));
    }
    if world.resource::<ParticleEffects>() != Some(&effects) {
        world.insert_resource(effects);
    }
    let Some(renderer) = renderer else { return };
    let mut pictures = ParticlePictures::default();
    for guid in wanted {
        match content.particle_picture(renderer, guid) {
            Ok(texture) => drop(pictures.0.insert(guid, texture)),
            Err(error) => tracing::debug!("a Particle System's picture {guid} could not be read: {error:#}"),
        }
    }
    if world.resource::<ParticlePictures>() != Some(&pictures) {
        world.insert_resource(pictures);
    }
}

/// Sets off a blast at `at`, `radius` metres wide: the particles of every
/// Particle System that takes forces are thrown out from it, those at its
/// middle to `speed` metres a second. Over in a moment
/// (`voxear_render::BLAST_TIME`).
pub fn blast(world: &mut World, at: voxear_core::math::Vec3, radius: f32, speed: f32) {
    if !(radius > 0.0 && speed > 0.0 && at.is_finite()) {
        return;
    }
    let started = world.resource::<ParticleClock>().map_or(0.0, |clock| clock.0) as f32;
    let blast = Blast { at, radius, speed, started };
    match world.resource_mut::<ParticleBlasts>() {
        Some(blasts) => blasts.0.push(blast),
        None => world.insert_resource(ParticleBlasts(vec![blast])),
    }
}

// What `field_values` reads or writes, as `ForceField` in Voxear.Core numbers them.
const FIELD_KIND: i32 = 3;
const FIELD_STRENGTH: i32 = 4;
const FIELD_RADIUS: i32 = 5;

/// Reads value `what` of `entity`'s Force Field, or with `written`, writes
/// it: `None` when it has none, or is gone.
fn field_values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<ForceField>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<ForceField>(entity) {
                world.insert(entity, ForceField::default());
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => return world.remove::<ForceField>(entity).map(|_| [0.0; 4]),
        _ => {}
    }
    let field = world.get_mut::<ForceField>(entity)?;
    match (what, written) {
        (FIELD_KIND, None) => {
            Some(number(ForceKind::ALL.iter().position(|kind| *kind == field.kind).unwrap_or_default() as f32))
        }
        (FIELD_STRENGTH, None) => Some(number(field.strength)),
        (FIELD_RADIUS, None) => Some(number(field.radius)),
        (FIELD_KIND, Some([x, ..])) => {
            field.kind = ForceKind::ALL[(x.max(0.0) as usize).min(ForceKind::ALL.len() - 1)];
            Some([0.0; 4])
        }
        (FIELD_STRENGTH, Some([x, ..])) => {
            field.strength = x;
            Some([0.0; 4])
        }
        (FIELD_RADIUS, Some([x, ..])) => {
            field.radius = x.max(0.0);
            Some([0.0; 4])
        }
        _ => None,
    }
}

/// Starts every Particle System that has not started yet, at `now`.
pub fn start(world: &mut World, now: f64) {
    world.insert_resource(ParticleClock(now));
    // Blasts that are over are forgotten.
    if let Some(blasts) = world.resource_mut::<ParticleBlasts>() {
        blasts.keep_live(now as f32);
    }
    let waiting: Vec<Entity> = world
        .query_ref::<(Entity, &ParticleSystem)>()
        .without::<ParticlePlayback>()
        .into_iter()
        .map(|(entity, _)| entity)
        .collect();
    for entity in waiting {
        world.insert(entity, ParticlePlayback { started: now as f32, stopped: None });
    }
}

/// Reads value `what` of `entity`'s Particle System, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let now = world.resource::<ParticleClock>().map_or(0.0, |clock| clock.0) as f32;
    match (what, written) {
        (HAS, None) => return Some(flag(world.has::<ParticleSystem>(entity))),
        (ADD, Some(_)) => {
            if !world.has::<ParticleSystem>(entity) {
                world.insert(entity, ParticleSystem::default());
                world.insert(entity, ParticlePlayback { started: now, stopped: None });
            }
            return Some([0.0; 4]);
        }
        (REMOVE, Some(_)) => {
            world.remove::<ParticlePlayback>(entity);
            return world.remove::<ParticleSystem>(entity).map(|_| [0.0; 4]);
        }
        _ => {}
    }
    world.get::<ParticleSystem>(entity)?;
    match (what, written) {
        // Over again from the start, or no more particles sent out, those out living on.
        (PLAY, Some(_)) => drop(world.insert(entity, ParticlePlayback { started: now, stopped: None })),
        (STOP, Some(_)) => {
            let playback = world.get_mut::<ParticlePlayback>(entity)?;
            playback.stopped = playback.stopped.or(Some(now));
        }
        (PLAYING, None) => return Some(flag(world.get::<ParticlePlayback>(entity).is_some_and(|p| p.stopped.is_none()))),
        (_, None) => return read(world.get::<ParticleSystem>(entity)?, what),
        (_, Some(value)) => write(world.get_mut::<ParticleSystem>(entity)?, what, value),
    }
    Some(written.unwrap_or_default())
}

fn read(system: &ParticleSystem, what: i32) -> Option<[f32; 4]> {
    let color = |c: [f32; 4]| {
        let [r, g, b] = srgb([c[0], c[1], c[2]]);
        [r, g, b, c[3]]
    };
    let pair = |p: [f32; 2]| [p[0], p[1], 0.0, 0.0];
    Some(match what {
        DURATION => number(system.duration),
        LOOPING => flag(system.looping),
        RATE => number(system.rate),
        BURST => number(system.burst as f32),
        LIFETIME => pair(system.lifetime),
        SPEED => pair(system.speed),
        SIZE => pair(system.size),
        SIZE_END => number(system.size_end),
        COLOR => color(system.color),
        COLOR_END => color(system.color_end),
        INTENSITY => number(system.intensity),
        SHAPE => number(EmitShape::ALL.iter().position(|s| *s == system.shape).unwrap_or_default() as f32),
        RADIUS => number(system.radius),
        ANGLE => number(system.angle),
        BOX_SIZE => {
            let [x, y, z] = system.box_size;
            [x, y, z, 0.0]
        }
        GRAVITY => number(system.gravity),
        DRAG => number(system.drag),
        SPIN => number(system.spin),
        BLEND => number(ParticleBlend::ALL.iter().position(|b| *b == system.blend).unwrap_or_default() as f32),
        LIT => flag(system.lit),
        SOFTNESS => number(system.softness),
        SEED => number(system.seed as f32),
        SPACE => number(ParticleSpace::ALL.iter().position(|s| *s == system.space).unwrap_or_default() as f32),
        WIND => number(system.wind),
        TURBULENCE => number(system.turbulence),
        TURBULENCE_SIZE => number(system.turbulence_size),
        SIMULATED => flag(system.is_simulated()),
        COLLISION => {
            number(ParticleCollision::ALL.iter().position(|c| *c == system.collision).unwrap_or_default() as f32)
        }
        BOUNCE => number(system.bounce),
        FRICTION => number(system.friction),
        LIFE_LOST => number(system.life_lost),
        FORCES => number(system.forces),
        STRETCH => number(system.stretch),
        DELAY => number(system.delay),
        _ => return None,
    })
}

fn write(system: &mut ParticleSystem, what: i32, [x, y, z, w]: [f32; 4]) {
    let color = || {
        let [r, g, b] = linear([x, y, z].map(|c| c.max(0.0)));
        [r, g, b, w.clamp(0.0, 1.0)]
    };
    let whole = || x.max(0.0).round() as u32;
    let pick = |count: usize| (x.max(0.0) as usize).min(count - 1);
    match what {
        DURATION => system.duration = x.max(0.01),
        LOOPING => system.looping = x != 0.0,
        RATE => system.rate = x.max(0.0),
        BURST => system.burst = whole(),
        LIFETIME => system.lifetime = [x.max(0.01), y.max(0.01)],
        SPEED => system.speed = [x, y],
        SIZE => system.size = [x.max(0.0), y.max(0.0)],
        SIZE_END => system.size_end = x.max(0.0),
        COLOR => system.color = color(),
        COLOR_END => system.color_end = color(),
        INTENSITY => system.intensity = x.max(0.0),
        SHAPE => system.shape = EmitShape::ALL[pick(EmitShape::ALL.len())],
        RADIUS => system.radius = x.max(0.0),
        ANGLE => system.angle = x.clamp(0.0, 180.0),
        BOX_SIZE => system.box_size = [x, y, z].map(|s| s.max(0.0)),
        GRAVITY => system.gravity = x,
        DRAG => system.drag = x.max(0.0),
        SPIN => system.spin = x,
        BLEND => system.blend = ParticleBlend::ALL[pick(ParticleBlend::ALL.len())],
        LIT => system.lit = x != 0.0,
        SOFTNESS => system.softness = x.max(0.0),
        SEED => system.seed = whole(),
        SPACE => system.space = ParticleSpace::ALL[pick(ParticleSpace::ALL.len())],
        WIND => system.wind = x.max(0.0),
        TURBULENCE => system.turbulence = x.max(0.0),
        TURBULENCE_SIZE => system.turbulence_size = x.max(0.01),
        COLLISION => system.collision = ParticleCollision::ALL[pick(ParticleCollision::ALL.len())],
        BOUNCE => system.bounce = x.clamp(0.0, 1.0),
        FRICTION => system.friction = x.clamp(0.0, 1.0),
        LIFE_LOST => system.life_lost = x.clamp(0.0, 1.0),
        FORCES => system.forces = x.max(0.0),
        STRETCH => system.stretch = x.max(0.0),
        DELAY => system.delay = x.max(0.0),
        _ => {}
    }
}

fn number(value: f32) -> [f32; 4] {
    [value, 0.0, 0.0, 0.0]
}

fn flag(on: bool) -> [f32; 4] {
    number(if on { 1.0 } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_particle_system_starts_as_it_appears_and_keeps_its_start() {
        let mut world = World::new();
        let first = world.spawn((ParticleSystem::default(),));
        start(&mut world, 2.0);
        let second = world.spawn((ParticleSystem::default(),));
        start(&mut world, 5.0);
        let started = |entity| world.get::<ParticlePlayback>(entity).map(|p| p.started);
        assert_eq!((started(first), started(second)), (Some(2.0), Some(5.0)));
    }

    #[test]
    fn scripts_change_a_system_and_play_and_stop_it_on_the_engines_clock() {
        let mut world = World::new();
        let obj = world.spawn(());
        start(&mut world, 1.0);
        assert_eq!(values(&mut world, obj, HAS, None), Some([0.0; 4]));
        assert_eq!(values(&mut world, obj, RATE, None), None, "nothing to read without one");
        values(&mut world, obj, ADD, Some([0.0; 4]));
        values(&mut world, obj, LIFETIME, Some([0.5, 4.0, 0.0, 0.0]));
        values(&mut world, obj, SHAPE, Some([1.0, 0.0, 0.0, 0.0]));
        // Mid grey as scripts give it is a fifth of white's light, kept with its opacity.
        values(&mut world, obj, COLOR, Some([0.5, 0.5, 0.5, 0.25]));
        let system = *world.get::<ParticleSystem>(obj).unwrap();
        assert_eq!((system.lifetime, system.shape), ([0.5, 4.0], EmitShape::Sphere));
        assert!((system.color[0] - 0.214).abs() < 0.01 && system.color[3] == 0.25, "{:?}", system.color);
        let back = values(&mut world, obj, COLOR, None).unwrap();
        assert!((back[0] - 0.5).abs() < 1e-3, "read back as given: {back:?}");
        start(&mut world, 3.0);
        values(&mut world, obj, STOP, Some([0.0; 4]));
        assert_eq!(world.get::<ParticlePlayback>(obj).unwrap().stopped, Some(3.0));
        assert_eq!(values(&mut world, obj, PLAYING, None), Some([0.0; 4]));
        start(&mut world, 4.0);
        values(&mut world, obj, PLAY, Some([0.0; 4]));
        assert_eq!(*world.get::<ParticlePlayback>(obj).unwrap(), ParticlePlayback { started: 4.0, stopped: None });
        assert_eq!(values(&mut world, obj, PLAYING, None), Some([1.0, 0.0, 0.0, 0.0]));
        // Left in the world and blown about, it is simulated; as it was, it is not.
        assert_eq!(values(&mut world, obj, SIMULATED, None), Some([0.0; 4]));
        values(&mut world, obj, SPACE, Some([1.0, 0.0, 0.0, 0.0]));
        values(&mut world, obj, WIND, Some([0.8, 0.0, 0.0, 0.0]));
        values(&mut world, obj, TURBULENCE_SIZE, Some([0.0; 4]));
        let system = *world.get::<ParticleSystem>(obj).unwrap();
        assert_eq!((system.space, system.wind, system.turbulence_size), (ParticleSpace::World, 0.8, 0.01));
        assert_eq!(values(&mut world, obj, SIMULATED, None), Some([1.0, 0.0, 0.0, 0.0]));
        values(&mut world, obj, COLLISION, Some([1.0, 0.0, 0.0, 0.0]));
        values(&mut world, obj, BOUNCE, Some([2.0, 0.0, 0.0, 0.0]));
        values(&mut world, obj, LIFE_LOST, Some([0.25, 0.0, 0.0, 0.0]));
        let system = *world.get::<ParticleSystem>(obj).unwrap();
        assert_eq!((system.collision, system.bounce, system.life_lost), (ParticleCollision::Depth, 1.0, 0.25));
    }

    #[test]
    fn scripts_set_off_blasts_that_are_forgotten_once_over_and_change_force_fields() {
        let mut world = World::new();
        start(&mut world, 5.0);
        blast(&mut world, voxear_core::math::Vec3::new(1.0, 2.0, 3.0), 4.0, 6.0);
        blast(&mut world, voxear_core::math::Vec3::ZERO, 0.0, 6.0);
        let blasts = world.resource::<ParticleBlasts>().unwrap().0.clone();
        assert_eq!(blasts.len(), 1, "one of no width sets nothing off");
        assert_eq!((blasts[0].started, blasts[0].radius, blasts[0].speed), (5.0, 4.0, 6.0));
        start(&mut world, 5.1);
        assert_eq!(world.resource::<ParticleBlasts>().unwrap().0.len(), 1, "still going off");
        start(&mut world, 5.0 + f64::from(voxear_render::BLAST_TIME) + 0.01);
        assert!(world.resource::<ParticleBlasts>().unwrap().0.is_empty(), "over, and forgotten");

        let obj = world.spawn(());
        assert_eq!(field_values(&mut world, obj, FIELD_RADIUS, None), None, "nothing to read without one");
        field_values(&mut world, obj, ADD, Some([0.0; 4]));
        field_values(&mut world, obj, FIELD_KIND, Some([2.0, 0.0, 0.0, 0.0]));
        field_values(&mut world, obj, FIELD_RADIUS, Some([-3.0, 0.0, 0.0, 0.0]));
        let field = *world.get::<ForceField>(obj).unwrap();
        assert_eq!((field.kind, field.radius), (ForceKind::Vortex, 0.0));
        assert_eq!(field_values(&mut world, obj, FIELD_KIND, None), Some([2.0, 0.0, 0.0, 0.0]));
    }
}
