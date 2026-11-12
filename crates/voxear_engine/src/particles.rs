//! Playing Particle Systems: each starts as it appears, its time counted
//! from then (`voxear_render::ParticlePlayback`), and scripts play and stop
//! it. For C#, `ParticleSystem` in Voxear.Core reads and writes an Obj's
//! system through one call, by number, as `lights` does a Light's.
//!
//! An effect is also played at a place, without an Obj made for it first
//! ([`play`], `Particles.Play` in C#): it gets an Obj of its own, which is
//! taken away when the effect is over.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    Blast, EffectRef, EmitShape, ForceField, ForceKind, ParticleBlasts, ParticleBlend, ParticleCollision,
    ParticlePlayback, ParticleSpace, ParticleSystem, ParticleTrails,
};
use voxear_scene::{Name, Parent, Transform};

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
const LIGHT: i32 = 40;
const LIGHT_RANGE: i32 = 41;
const TRAILS: i32 = 42;
const TRAIL_TIME: i32 = 43;
const TRAIL_WIDTH: i32 = 44;
const HIT_EVENTS: i32 = 45;

/// The time Particle Systems are played and stopped by, and blasts go off
/// in: the engine's, but for how the editor's preview holds it back; and
/// the engine's time when it was last brought up to date.
#[derive(Clone, Copy, Debug, Default)]
struct ParticleClock {
    now: f64,
    engine: Option<f64>,
}

/// How fast particles' time runs against the engine's, a resource of the
/// world: paused, or at a share of full speed, as the editor's preview
/// asks. Without one, at full speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticlePreview {
    pub paused: bool,
    pub speed: f32,
}

impl Default for ParticlePreview {
    fn default() -> Self {
        Self { paused: false, speed: 1.0 }
    }
}

/// The time Particle Systems are played by now, which a `ParticlePlayback`
/// started at is in.
pub fn now(world: &World) -> f64 {
    world.resource::<ParticleClock>().map_or(0.0, |clock| clock.now)
}

/// An effect as a scene, to open on its own and edit as one: an Obj an
/// emitter, in the effect's order, named for it, with its settings as its
/// Particle System.
pub(crate) fn effect_scene(effect: &voxear_render::ParticleEffect) -> crate::scene_file::SceneFile {
    let objs = effect.emitters.iter().zip(1..).map(|(emitter, id)| crate::scene_file::ObjData {
        particles: Some(emitter.system),
        ..crate::scene_file::ObjData::new(id, &emitter.name)
    });
    crate::scene_file::SceneFile { objs: objs.collect(), ..Default::default() }
}

/// The effect a scene opened from one is now: each of its Objs that has a
/// Particle System an emitter, in the scene's order. Where the Objs stand
/// and what else they have is not part of an effect.
pub(crate) fn scene_effect(scene: &crate::scene_file::SceneFile) -> voxear_render::ParticleEffect {
    let emitters = scene.objs.iter().filter_map(|obj| {
        let system = ParticleSystem { effect: None, ..obj.particles? };
        Some(voxear_render::Emitter { name: obj.name.clone(), system })
    });
    voxear_render::ParticleEffect { emitters: emitters.collect() }
}

/// Strikes particles made, read back by the renderer and not yet told to
/// scripts (see [`take_hits`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleHits(pub Vec<voxear_render::ParticleHit>);

/// The strikes waiting to be told to scripts, as they are told of them;
/// those of systems that have gone are left out.
pub fn take_hits(world: &mut World) -> Vec<voxear_scripting::ParticleHitData> {
    let Some(ParticleHits(hits)) = world.remove_resource::<ParticleHits>() else { return Vec::new() };
    let told = hits.into_iter().filter(|hit| world.is_alive(hit.entity));
    told.map(|hit| voxear_scripting::ParticleHitData {
        entity: hit.entity.to_bits(),
        emitter: hit.emitter,
        speed: hit.speed,
        point: hit.position.to_array(),
        normal: hit.normal.to_array(),
    })
    .collect()
}

/// A Particle System that [`play`] made to play an effect at a place: its
/// Obj is taken away once the effect is over, the last of its particles
/// gone; one that loops, once it has been stopped and its last have gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayedEffect;

impl voxear_ecs::Component for PlayedEffect {}

/// How many effects [`play`] has started, which gives each a seed of its
/// own.
#[derive(Clone, Copy, Debug, Default)]
struct Plays(u32);

/// How long past its end a played effect's Obj is kept, in seconds; and how
/// long one whose effect cannot be read is, before it is given up on.
const PLAYED_OUT: f32 = 0.5;
const UNREAD_FOR: f32 = 10.0;

/// Plays `effect` at `at`, turned by `rotation`, on an Obj of its own that
/// is taken away when the effect is over: an explosion where a shell lands,
/// a flash at a muzzle. Under `parent`, the place is in the parent's space
/// and the effect goes where the parent goes. Each play sends out particles
/// of its own, or with a `seed`, the same as every other play with that
/// seed, as every machine of a networked game must.
pub fn play(
    world: &mut World,
    effect: EffectRef,
    (at, rotation): (Vec3, Quat),
    parent: Option<Entity>,
    seed: Option<u32>,
) -> Entity {
    let plays = world.resource::<Plays>().map_or(0, |plays| plays.0);
    world.insert_resource(Plays(plays.wrapping_add(1)));
    let seed = seed.unwrap_or(plays.wrapping_mul(0x9E37_79B9).wrapping_add(1));
    let system = ParticleSystem { effect: Some(effect), seed, ..ParticleSystem::default() };
    let playback = ParticlePlayback { started: now(world) as f32, stopped: None };
    let name = match effect {
        EffectRef::Builtin(starter) => starter.name(),
        EffectRef::Asset(_) => "Effect",
    };
    let transform = Transform::new(at, rotation, Vec3::ONE);
    let entity = world.spawn((Name(name.to_string()), system, playback, PlayedEffect, transform));
    if let Some(parent) = parent.filter(|parent| world.is_alive(*parent)) {
        world.insert(entity, Parent(parent));
    }
    entity
}

/// Whether a played effect is over `now`: the last of its particles gone,
/// since it ended or since it was stopped; or its effect still not read
/// long after it started.
fn played_out(
    library: Option<&voxear_render::ParticleEffects>,
    system: &ParticleSystem,
    playback: &ParticlePlayback,
    now: f32,
) -> bool {
    let since = now - playback.started;
    let Some(longest) = voxear_render::particles_longest_life(library, system) else { return since > UNREAD_FOR };
    let ended = voxear_render::particles_end_after(library, system).is_some_and(|end| since > end + PLAYED_OUT);
    ended || playback.stopped.is_some_and(|stopped| now > stopped + longest + PLAYED_OUT)
}

/// What scripts ask of Particle Systems in words: the effect one plays, by
/// its reference, read or set; and an effect played at a place.
const EFFECT: i32 = 0;
const SET_EFFECT: i32 = 1;
const PLAY_EFFECT: i32 = 2;
/// A curve or the gradient, read; and, CHANGE on from it, set. Keys a line
/// each, where then what; a gradient's colour as it looks on screen, sRGB.
const SIZE_CURVE: i32 = 10;
const OPACITY_CURVE: i32 = 11;
const SPEED_CURVE: i32 = 12;
const COLOR_GRADIENT: i32 = 13;
const CHANGE: i32 = 10;

/// A curve's or the gradient's keys as words, a line each.
fn keys_text<const N: usize>(keys: &[[f32; N]]) -> String {
    let line = |key: &[f32; N]| key.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ");
    keys.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// Keys from words, a line each of `N` numbers; `None` for any line that is not.
fn text_keys<const N: usize>(text: &str) -> Option<Vec<[f32; N]>> {
    let line = |line: &str| -> Option<[f32; N]> {
        let numbers: Vec<f32> = line.split_whitespace().map(|n| n.parse().ok().filter(|n: &f32| n.is_finite())).collect::<Option<_>>()?;
        numbers.try_into().ok()
    };
    text.lines().filter(|line| !line.trim().is_empty()).map(line).collect()
}

/// Reads, or with `text` sets, a curve or the gradient of `system`.
fn curve_text(system: &mut ParticleSystem, which: i32, text: Option<&str>) -> Option<String> {
    use voxear_core::curve::{Curve, Gradient};
    let curve = match which {
        SIZE_CURVE => &mut system.size_curve,
        OPACITY_CURVE => &mut system.opacity_curve,
        SPEED_CURVE => &mut system.speed_curve,
        COLOR_GRADIENT => {
            let Some(text) = text else {
                let shown = system.color_gradient.keys().iter().map(|&[t, r, g, b]| {
                    let [r, g, b] = srgb([r, g, b]);
                    [t, r, g, b]
                });
                return Some(keys_text(&shown.collect::<Vec<_>>()));
            };
            let keys = text_keys::<4>(text)?.into_iter().map(|[t, r, g, b]| {
                let [r, g, b] = linear([r, g, b].map(|c| c.max(0.0)));
                [t, r, g, b]
            });
            system.color_gradient = Gradient::new(&keys.collect::<Vec<_>>());
            return Some(String::new());
        }
        _ => return None,
    };
    match text {
        None => Some(keys_text(curve.keys())),
        Some(text) => {
            *curve = Curve::new(&text_keys::<2>(text)?);
            Some(String::new())
        }
    }
}

/// Answers scripts' asks in words (see [`EFFECT`]). To play an effect the
/// text is four lines: the effect's reference, where, how it is turned, and
/// its seed or nothing; `entity` is what it is played under, if alive; and
/// the answer is the new Obj's number.
fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    match what {
        EFFECT => {
            let effect = world.is_alive(entity).then(|| world.get::<ParticleSystem>(entity))??.effect;
            Some(effect.map_or(String::new(), |effect| effect.to_string()))
        }
        SET_EFFECT => {
            let effect = if text.is_empty() { None } else { Some(text.parse::<EffectRef>().ok()?) };
            let started = now(world) as f32;
            world.is_alive(entity).then(|| world.get_mut::<ParticleSystem>(entity))??.effect = effect;
            // Played from its start, so one that plays once is seen.
            world.insert(entity, ParticlePlayback { started, stopped: None });
            Some(String::new())
        }
        SIZE_CURVE..=COLOR_GRADIENT => {
            curve_text(world.is_alive(entity).then(|| world.get_mut::<ParticleSystem>(entity))??, what, None)
        }
        _ if (SIZE_CURVE + CHANGE..=COLOR_GRADIENT + CHANGE).contains(&what) => {
            let system = world.is_alive(entity).then(|| world.get_mut::<ParticleSystem>(entity))??;
            curve_text(system, what - CHANGE, Some(text))
        }
        PLAY_EFFECT => {
            let mut lines = text.lines();
            let effect = lines.next()?.parse::<EffectRef>().ok()?;
            let mut numbers = |count: usize| -> Option<Vec<f32>> {
                let read: Vec<f32> = lines.next()?.split_whitespace().filter_map(|n| n.parse().ok()).collect();
                (read.len() == count && read.iter().all(|n| n.is_finite())).then_some(read)
            };
            let at = Vec3::from_slice(&numbers(3)?);
            let rotation = Quat::from_slice(&numbers(4)?).normalize();
            let seed = lines.next().and_then(|seed| seed.trim().parse::<u32>().ok());
            let parent = world.is_alive(entity).then_some(entity);
            Some(play(world, effect, (at, rotation), parent, seed).to_bits().to_string())
        }
        _ => None,
    }
}

/// Sets what answers `ParticleSystem`, `ForceField`, `Particles.Push` and
/// `Particles.Play` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_kind_texts(voxear_scripting::ComponentKind::Particles, texts);
    voxear_scripting::set_particle_values(values);
    voxear_scripting::set_kind_values(voxear_scripting::ComponentKind::ForceField, field_values);
    voxear_scripting::set_particle_blaster(|world, at, radius, speed| blast(world, at.into(), radius, speed));
}

/// Reads what the world's Particle Systems name, for the renderer to find
/// (see `voxear_render::effects`): the effects they play, from the
/// project's `.particles` assets, and the pictures and the models' meshes
/// they and those effects' emitters draw with, which need `renderer` to
/// load. Each is read once and
/// kept by `content`, so this is a few lookups a system each frame, and an
/// effect whose file changed is taken up as soon as `content` forgets it.
pub fn resolve(world: &mut World, content: &mut crate::content::Content, renderer: Option<&mut voxear_render::Renderer>) {
    use voxear_render::{EffectRef, ParticleEffects, ParticleMesh, ParticleMeshes, ParticlePictures, ParticleTexture};
    let mut effects = ParticleEffects::default();
    let mut wanted = std::collections::HashSet::new();
    let mut models = std::collections::HashSet::new();
    for (system,) in world.query_ref::<(&ParticleSystem,)>() {
        let (emitters, meshes): (Vec<ParticleTexture>, Vec<Option<ParticleMesh>>) = match system.effect {
            None => (vec![system.texture, system.light_map], vec![system.mesh]),
            Some(EffectRef::Builtin(_)) => continue,
            Some(EffectRef::Asset(guid)) => {
                let found = effects.0.get(&guid).cloned().or_else(|| content.particle_effect(guid));
                let Some(effect) = found else { continue };
                let both = |emitter: &voxear_render::Emitter| [emitter.system.texture, emitter.system.light_map];
                let textures = effect.emitters.iter().flat_map(both).collect();
                let meshes = effect.emitters.iter().map(|emitter| emitter.system.mesh).collect();
                effects.0.insert(guid, effect);
                (textures, meshes)
            }
        };
        wanted.extend(emitters.into_iter().filter_map(|texture| match texture {
            ParticleTexture::Asset(guid) => Some(guid),
            ParticleTexture::Builtin(_) => None,
        }));
        models.extend(meshes.into_iter().filter_map(|mesh| match mesh? {
            ParticleMesh::Asset(guid) => Some(guid),
            ParticleMesh::Builtin(_) => None,
        }));
    }
    if world.resource::<ParticleEffects>() != Some(&effects) {
        world.insert_resource(effects);
    }
    let Some(renderer) = renderer else { return };
    // The strikes read back since last frame, for scripts to hear of this one.
    let struck = renderer.take_particle_hits();
    if !struck.is_empty() {
        let mut hits = world.remove_resource::<ParticleHits>().unwrap_or_default();
        hits.0.extend(struck);
        let over = hits.0.len().saturating_sub(voxear_render::MAX_PARTICLE_HITS as usize * 4);
        hits.0.drain(..over);
        world.insert_resource(hits);
    }
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
    let mut meshes = ParticleMeshes::default();
    for guid in models {
        match content.model(renderer, guid).map(|nodes| nodes.first().map(|node| (node.mesh, node.material))) {
            Ok(Some(mesh)) => drop(meshes.0.insert(guid, mesh)),
            Ok(None) => tracing::debug!("a Particle System's model {guid} has no mesh"),
            Err(error) => tracing::debug!("a Particle System's model {guid} could not be read: {error:#}"),
        }
    }
    if world.resource::<ParticleMeshes>() != Some(&meshes) {
        world.insert_resource(meshes);
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
    let started = now(world) as f32;
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
pub fn start(world: &mut World, engine: f64) {
    // The particles' time follows the engine's, as fast as the preview lets it; it starts as the engine's.
    let clock = world.resource::<ParticleClock>().copied().unwrap_or_default();
    let preview = world.resource::<ParticlePreview>().copied().unwrap_or_default();
    let passed = clock.engine.map_or(0.0, |last| (engine - last).max(0.0));
    let rate = if preview.paused { 0.0 } else { f64::from(preview.speed.max(0.0)) };
    let now = if clock.engine.is_some() { clock.now + passed * rate } else { engine };
    world.insert_resource(ParticleClock { now, engine: Some(engine) });
    world.insert_resource(voxear_render::ParticleTime(now as f32));
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
    // Effects played at a place that are over are taken away, their Objs with them.
    let library = world.resource::<voxear_render::ParticleEffects>();
    let over: Vec<Entity> = world
        .query_ref::<(Entity, &ParticleSystem, &ParticlePlayback, &PlayedEffect)>()
        .into_iter()
        .filter(|(_, system, playback, _)| played_out(library, system, playback, now as f32))
        .map(|(entity, ..)| entity)
        .collect();
    for entity in over {
        world.despawn(entity);
    }
}

/// Reads value `what` of `entity`'s Particle System, or with `written`,
/// writes it: `None` when it has none, or is gone.
fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let now = now(world) as f32;
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
        LIGHT => number(system.light),
        LIGHT_RANGE => number(system.light_range),
        TRAILS => number(ParticleTrails::ALL.iter().position(|t| *t == system.trails).unwrap_or_default() as f32),
        TRAIL_TIME => number(system.trail_time),
        TRAIL_WIDTH => number(system.trail_width),
        HIT_EVENTS => flag(system.hit_events),
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
        LIGHT => system.light = x.max(0.0),
        LIGHT_RANGE => system.light_range = x.max(0.0),
        TRAILS => system.trails = ParticleTrails::ALL[pick(ParticleTrails::ALL.len())],
        TRAIL_TIME => system.trail_time = x.max(0.01),
        TRAIL_WIDTH => system.trail_width = x.max(0.0),
        HIT_EVENTS => system.hit_events = x != 0.0,
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
        // Trails by their place among the kinds, a tail never of no length.
        values(&mut world, obj, TRAILS, Some([1.0, 0.0, 0.0, 0.0]));
        values(&mut world, obj, TRAIL_TIME, Some([0.0; 4]));
        values(&mut world, obj, TRAIL_WIDTH, Some([0.75, 0.0, 0.0, 0.0]));
        let system = *world.get::<ParticleSystem>(obj).unwrap();
        assert_eq!((system.trails, system.trail_time, system.trail_width), (ParticleTrails::Particles, 0.01, 0.75));
        assert_eq!(values(&mut world, obj, TRAILS, None), Some([1.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn an_effect_played_at_a_place_has_an_obj_of_its_own_that_goes_when_it_is_over() {
        use voxear_render::Starter;
        let mut world = World::new();
        start(&mut world, 5.0);
        let explosion = EffectRef::Builtin(Starter::Explosion);
        let upright = Quat::IDENTITY;
        let first = play(&mut world, explosion, (Vec3::new(1.0, 2.0, 3.0), upright), None, None);
        let second = play(&mut world, explosion, (Vec3::ZERO, upright), None, None);
        let (a, b) = (*world.get::<ParticleSystem>(first).unwrap(), *world.get::<ParticleSystem>(second).unwrap());
        assert_eq!((a.effect, world.get::<Transform>(first).unwrap().position), (Some(explosion), Vec3::new(1.0, 2.0, 3.0)));
        assert_eq!(world.get::<ParticlePlayback>(first), Some(&ParticlePlayback { started: 5.0, stopped: None }));
        assert_ne!(a.seed, b.seed, "each play its own particles");
        let seeded = play(&mut world, explosion, (Vec3::ZERO, upright), None, Some(7));
        assert_eq!(world.get::<ParticleSystem>(seeded).unwrap().seed, 7, "unless told which");
        // Gone once the last of its particles has died, and not before.
        let end = f64::from(voxear_render::particles_end_after(None, &a).expect("an explosion ends"));
        start(&mut world, 5.0 + end);
        assert!(world.is_alive(first) && world.is_alive(second));
        start(&mut world, 5.0 + end + 1.0);
        assert!(!world.is_alive(first) && !world.is_alive(second) && !world.is_alive(seeded));
        // One that loops stays until it is stopped, then as long as its longest lived lives.
        let fire = play(&mut world, EffectRef::Builtin(Starter::Fire), (Vec3::ZERO, upright), None, None);
        let longest = f64::from(voxear_render::particles_longest_life(None, world.get::<ParticleSystem>(fire).unwrap()).unwrap());
        start(&mut world, 1000.0);
        assert!(world.is_alive(fire), "a fire burns on");
        values(&mut world, fire, STOP, Some([0.0; 4]));
        start(&mut world, 1000.0 + longest);
        assert!(world.is_alive(fire), "its last smoke still out");
        start(&mut world, 1000.0 + longest + 1.0);
        assert!(!world.is_alive(fire));
        // Under an Obj, it is the Obj's child; and a system nobody played is left alone however long it has run.
        let gun = world.spawn((Transform::default(),));
        let own = world.spawn((ParticleSystem { looping: false, ..ParticleSystem::default() },));
        let flash = play(&mut world, EffectRef::Builtin(Starter::MuzzleFlash), (Vec3::ZERO, upright), Some(gun), None);
        assert_eq!(world.get::<Parent>(flash).map(|parent| parent.0), Some(gun));
        // An effect that cannot be read is given up on after a while.
        let missing = EffectRef::Asset(voxear_assets::AssetGuid::new_random());
        let lost = play(&mut world, missing, (Vec3::ZERO, upright), None, None);
        start(&mut world, 1005.0 + longest);
        assert!(world.is_alive(lost) && world.is_alive(own));
        start(&mut world, 1020.0 + longest);
        assert!(!world.is_alive(lost) && !world.is_alive(flash) && world.is_alive(own));
    }

    #[test]
    fn scripts_read_and_set_curves_and_the_gradient_in_words_and_hear_of_hits_when_asked() {
        let mut world = World::new();
        let obj = world.spawn((ParticleSystem::default(),));
        assert_eq!(texts(&mut world, obj, SIZE_CURVE, ""), Some(String::new()), "no keys yet");
        assert_eq!(texts(&mut world, obj, SIZE_CURVE + CHANGE, "1 0\n0 0.5\n0.25 1"), Some(String::new()));
        let system = *world.get::<ParticleSystem>(obj).unwrap();
        assert_eq!(system.size_curve.keys(), [[0.0, 0.5], [0.25, 1.0], [1.0, 0.0]], "put in order");
        assert_eq!(texts(&mut world, obj, SIZE_CURVE, ""), Some("0 0.5\n0.25 1\n1 0".to_string()));
        assert_eq!(texts(&mut world, obj, SPEED_CURVE + CHANGE, "0 nothing"), None, "not numbers");
        assert_eq!(texts(&mut world, obj, OPACITY_CURVE + CHANGE, "0 1 2"), None, "too many");
        // The gradient is written as it looks and kept linear, as the colours are.
        assert_eq!(texts(&mut world, obj, COLOR_GRADIENT + CHANGE, "0 1 0.5 0\n1 0 0 1"), Some(String::new()));
        let gradient = world.get::<ParticleSystem>(obj).unwrap().color_gradient;
        assert!((gradient.keys()[0][2] - 0.214).abs() < 0.01 && gradient.keys()[1] == [1.0, 0.0, 0.0, 1.0], "{gradient:?}");
        let read = texts(&mut world, obj, COLOR_GRADIENT, "").unwrap();
        let back: Vec<f32> = read.split_whitespace().map(|n| n.parse().unwrap()).collect();
        assert!((back[2] - 0.5).abs() < 1e-4 && back.len() == 8, "read back as it looks: {read}");
        // Clearing takes every key away.
        assert_eq!(texts(&mut world, obj, SIZE_CURVE + CHANGE, ""), Some(String::new()));
        assert!(world.get::<ParticleSystem>(obj).unwrap().size_curve.is_empty());
        // Hit Events read and set by number.
        values(&mut world, obj, HIT_EVENTS, Some([1.0, 0.0, 0.0, 0.0]));
        assert!(world.get::<ParticleSystem>(obj).unwrap().hit_events);
        // Strikes are told only of systems still there.
        let gone = world.spawn((ParticleSystem::default(),));
        let hit = |entity| voxear_render::ParticleHit { entity, emitter: 1, position: Vec3::ONE, normal: Vec3::Y, speed: 4.0 };
        world.insert_resource(ParticleHits(vec![hit(obj), hit(gone)]));
        world.despawn(gone);
        let told = take_hits(&mut world);
        assert_eq!(told.len(), 1);
        assert_eq!((told[0].entity, told[0].emitter, told[0].speed, told[0].normal), (obj.to_bits(), 1, 4.0, [0.0, 1.0, 0.0]));
        assert!(take_hits(&mut world).is_empty(), "told once");
    }

    #[test]
    fn scripts_play_effects_and_set_a_systems_effect_in_words() {
        use voxear_render::Starter;
        let mut world = World::new();
        start(&mut world, 2.0);
        let nobody = Entity::from_bits(0);
        let asked = "builtin:explosion\n1 2.5 -3\n0 0 0 1\n9";
        let made = texts(&mut world, nobody, PLAY_EFFECT, asked).expect("it plays");
        let made = Entity::from_bits(made.parse().expect("the new Obj's number"));
        let system = *world.get::<ParticleSystem>(made).expect("with a Particle System");
        assert_eq!((system.effect, system.seed), (Some(EffectRef::Builtin(Starter::Explosion)), 9));
        assert_eq!(world.get::<Transform>(made).unwrap().position, Vec3::new(1.0, 2.5, -3.0));
        assert!(world.has::<PlayedEffect>(made) && !world.has::<Parent>(made));
        // No seed, a seed of its own; under an Obj, its child; what is no effect or no place plays nothing.
        let gun = world.spawn((Transform::default(),));
        let on_gun = texts(&mut world, gun, PLAY_EFFECT, "builtin:muzzle_flash\n0 0 0\n0 0 0 1\n").expect("it plays");
        let on_gun = Entity::from_bits(on_gun.parse().unwrap());
        assert_eq!(world.get::<Parent>(on_gun).map(|parent| parent.0), Some(gun));
        assert_eq!(texts(&mut world, nobody, PLAY_EFFECT, "builtin:nothing\n0 0 0\n0 0 0 1\n"), None);
        assert_eq!(texts(&mut world, nobody, PLAY_EFFECT, "builtin:fire\n0 0\n0 0 0 1\n"), None);
        // A system's effect, read and set by its reference; setting it plays it from its start.
        let obj = world.spawn((ParticleSystem::default(),));
        assert_eq!(texts(&mut world, obj, EFFECT, ""), Some(String::new()));
        start(&mut world, 6.0);
        assert_eq!(texts(&mut world, obj, SET_EFFECT, "builtin:fire"), Some(String::new()));
        assert_eq!(texts(&mut world, obj, EFFECT, ""), Some("builtin:fire".to_string()));
        assert_eq!(world.get::<ParticlePlayback>(obj).unwrap().started, 6.0);
        assert_eq!(texts(&mut world, obj, SET_EFFECT, ""), Some(String::new()));
        assert_eq!(world.get::<ParticleSystem>(obj).unwrap().effect, None);
        assert_eq!(texts(&mut world, obj, SET_EFFECT, "builtin:nothing"), None);
        assert_eq!((texts(&mut world, gun, EFFECT, ""), texts(&mut world, nobody, EFFECT, "")), (None, None));
    }

    #[test]
    fn particles_time_follows_the_engines_but_for_the_preview() {
        let mut world = World::new();
        start(&mut world, 10.0);
        assert_eq!(now(&world), 10.0, "it starts as the engine's");
        start(&mut world, 11.0);
        assert_eq!(now(&world), 11.0);
        world.insert_resource(ParticlePreview { paused: true, speed: 1.0 });
        start(&mut world, 14.0);
        assert_eq!(now(&world), 11.0, "paused, it stands still");
        world.insert_resource(ParticlePreview { paused: false, speed: 0.25 });
        start(&mut world, 18.0);
        assert_eq!(now(&world), 12.0, "at a quarter speed");
        assert_eq!(world.resource::<voxear_render::ParticleTime>().unwrap().0, 12.0, "as the renderer plays them");
        // A system that appears now starts now, by the particles' time.
        let obj = world.spawn((ParticleSystem::default(),));
        start(&mut world, 18.0);
        assert_eq!(world.get::<ParticlePlayback>(obj).unwrap().started, 12.0);
    }

    #[test]
    fn an_effect_opens_as_a_scene_of_its_emitters_and_goes_back_as_it_was() {
        use voxear_render::Starter;
        let effect = Starter::Explosion.effect();
        let scene = effect_scene(effect);
        let names: Vec<&str> = scene.objs.iter().map(|obj| obj.name.as_str()).collect();
        assert_eq!(names, ["Flash", "Fireball", "Smoke", "Sparks", "Debris", "Shockwave"]);
        assert_eq!(scene.objs[1].particles, Some(effect.emitters[1].system));
        assert_eq!(&scene_effect(&scene), effect);
        // An Obj with no Particle System is no emitter, and an emitter plays no effect of its own.
        let mut edited = effect_scene(effect);
        edited.objs.insert(0, crate::scene_file::ObjData::new(9, "Light"));
        edited.objs[1].particles.as_mut().unwrap().effect = Some(voxear_render::EffectRef::Builtin(Starter::Fire));
        let back = scene_effect(&edited);
        assert_eq!((back.emitters.len(), back.emitters[0].name.as_str()), (6, "Flash"));
        assert_eq!(back.emitters[0].system.effect, None);
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
