//! Weather where the game runs: turns the level's [`Weather`] as time
//! passes, and keeps its rain and snow falling round whoever is looking.
//!
//! What the weather does to the sky, the sun, the fog and the wind is the
//! renderer's (`voxear_render::weather`), laid over the level's lighting as
//! each frame is drawn. Rain and snow are particles: the starter Rain and
//! Snow effects, widened, sent out of a box that stays over the viewer, in
//! the world's space so what has fallen stays where it fell. Each is
//! [`LAYERS`] Particle Systems, on one after another as it rains harder: a
//! layer started or stopped lets its own drops finish, where one system
//! whose rate changed would start its drops afresh at every change. The
//! layers are Objs of the engine's own, in no scene, never saved or shown.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
pub use voxear_render::weather::{Weather, WeatherAmounts, WeatherKind};
use voxear_render::{ParticlePlayback, ParticleSpace, ParticleSystem, Starter};
use voxear_scene::{EditorOnly, Name, Transform};

use crate::particles::PlayedEffect;

/// How many Particle Systems rain is made of, and snow: as many are on as
/// the amount reaches into.
pub const LAYERS: usize = 4;
/// How wide the box they fall out of is, in metres.
const SIDE: f32 = 24.0;

/// One of the things that fall: the starter effect it is made from, how far
/// over the viewer its box is, in metres, far enough for it to reach the
/// ground round the viewer's feet before it is spent, and how many times
/// the starter's size each of its particles is.
#[derive(Clone, Copy)]
struct Fall {
    starter: Starter,
    above: f32,
    size: f32,
}

const RAIN: Fall = Fall { starter: Starter::Rain, above: 9.0, size: 1.0 };
/// The starter's flakes are a real flake's size, too small to see falling
/// at a distance; weather's are a game's.
const SNOW: Fall = Fall { starter: Starter::Snow, above: 7.0, size: 2.5 };
/// An amount below this is none.
const NONE: f32 = 0.002;

/// The Objs raining and snowing round the viewer, a layer each.
#[derive(Default)]
struct Falling {
    rain: [Option<Entity>; LAYERS],
    snow: [Option<Entity>; LAYERS],
}

/// Turns the world's weather by `delta` seconds, and has as much rain and
/// snow falling round the viewer as it now says: the terrain's viewer where
/// there is one, as the editor's Scene view is, or the game's camera.
pub fn update(world: &mut World, delta: f32) {
    let now = match world.resource_mut::<Weather>() {
        Some(weather) => {
            weather.advance(delta);
            weather.now()
        }
        None => WeatherAmounts::default(),
    };
    let had = world.resource::<Falling>().is_some();
    if !had && now.rain <= NONE && now.snow <= NONE {
        return;
    }
    let viewer = crate::terrain::viewer(world);
    let mut falling = world.remove_resource::<Falling>().unwrap_or_default();
    // With no one to see it, none falls.
    let (rain, snow) = if viewer.is_some() { (now.rain, now.snow) } else { (0.0, 0.0) };
    let viewer = viewer.unwrap_or_default();
    fall(world, &mut falling.rain, RAIN, rain, viewer);
    fall(world, &mut falling.snow, SNOW, snow, viewer);
    world.insert_resource(falling);
}

/// How many layers an amount has on.
pub fn layers_on(amount: f32) -> usize {
    if amount <= NONE { 0 } else { ((amount * LAYERS as f32).ceil() as usize).clamp(1, LAYERS) }
}

/// Has as many of `layers` falling as `amount` asks for, over `viewer`.
fn fall(world: &mut World, layers: &mut [Option<Entity>; LAYERS], what: Fall, amount: f32, viewer: Vec3) {
    let wanted = layers_on(amount);
    let time = crate::particles::now(world) as f32;
    let over = viewer + Vec3::Y * what.above;
    for (index, layer) in layers.iter_mut().enumerate() {
        let falling = layer.filter(|entity| world.is_alive(*entity));
        *layer = match (falling, index < wanted) {
            (None, true) => Some(start(world, what, index, (over, time))),
            (Some(entity), false) => {
                // Its last drops fall, and then its Obj is taken away as a played effect's is.
                if let Some(playback) = world.get_mut::<ParticlePlayback>(entity) {
                    playback.stopped = Some(time);
                }
                None
            }
            (falling, _) => falling,
        };
        if let Some(transform) = layer.and_then(|entity| world.get_mut::<Transform>(entity)) {
            transform.position = over;
        }
    }
}

/// One layer of rain or snow, starting now.
fn start(world: &mut World, what: Fall, index: usize, (over, time): (Vec3, f32)) -> Entity {
    let system = layer_system(what, index);
    let playback = ParticlePlayback { started: time, stopped: None };
    let name = Name(format!("Weather {}", what.starter.name()));
    world.spawn((name, system, playback, PlayedEffect, EditorOnly, Transform::new(over, Quat::IDENTITY, Vec3::ONE)))
}

/// The Particle System of one layer: the starter effect's, over a wider
/// box at as many more a second, a share of them to each layer, each layer
/// with drops of its own.
fn layer_system(what: Fall, index: usize) -> ParticleSystem {
    let base = what.starter.effect().emitters.first().map(|emitter| emitter.system).unwrap_or_default();
    let wider = (SIDE * SIDE) / (base.box_size[0] * base.box_size[2]).max(1.0);
    ParticleSystem {
        rate: base.rate * wider / LAYERS as f32,
        size: base.size.map(|size| size * what.size),
        box_size: [SIDE, base.box_size[1], SIDE],
        space: ParticleSpace::World,
        seed: 0x5EED_0000 + what.starter as u32 * 16 + index as u32,
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_render::Camera;

    fn falling(world: &World, name: &str) -> Vec<Entity> {
        let named = world.query_ref::<(Entity, &Name, &ParticlePlayback)>();
        named
            .into_iter()
            .filter(|(_, n, playback)| n.0 == name && playback.stopped.is_none())
            .map(|(e, ..)| e)
            .collect()
    }

    #[test]
    fn rain_falls_over_the_camera_as_hard_as_the_weather_says_and_stops_with_it() {
        let mut world = World::new();
        let camera = world.spawn((Camera::default(), Transform::from_position(Vec3::new(5.0, 2.0, -3.0))));
        update(&mut world, 0.1);
        assert!(falling(&world, "Weather Rain").is_empty(), "clear without any weather");

        world.insert_resource(Weather::of(WeatherKind::Rain));
        update(&mut world, 0.1);
        let rain = falling(&world, "Weather Rain");
        assert_eq!(rain.len(), layers_on(WeatherKind::Rain.amounts().rain));
        assert!(rain.len() < LAYERS && falling(&world, "Weather Snow").is_empty());
        let over = world.get::<Transform>(rain[0]).unwrap().position;
        assert_eq!(over, Vec3::new(5.0, 2.0 + RAIN.above, -3.0));
        assert!(world.get::<EditorOnly>(rain[0]).is_some(), "in no scene");

        // It follows the camera, and a storm turns every layer on as it comes.
        world.get_mut::<Transform>(camera).unwrap().position.x = 40.0;
        world.resource_mut::<Weather>().unwrap().set(WeatherKind::Storm, 10.0);
        update(&mut world, 1.0);
        assert_eq!(falling(&world, "Weather Rain").len(), rain.len(), "not yet");
        assert_eq!(world.get::<Transform>(rain[0]).unwrap().position.x, 40.0);
        update(&mut world, 9.0);
        assert_eq!(falling(&world, "Weather Rain").len(), LAYERS);

        // Snow in its place: the rain's layers are stopped, to finish and be taken away.
        world.resource_mut::<Weather>().unwrap().set(WeatherKind::Snow, 0.0);
        update(&mut world, 0.1);
        assert!(falling(&world, "Weather Rain").is_empty());
        assert_eq!(falling(&world, "Weather Snow").len(), layers_on(WeatherKind::Snow.amounts().snow));
        assert!(rain.iter().all(|&layer| world.get::<ParticlePlayback>(layer).unwrap().stopped.is_some()));
    }

    #[test]
    fn layers_fall_wide_in_the_worlds_space_each_with_drops_of_its_own() {
        let (first, second) = (layer_system(RAIN, 0), layer_system(RAIN, 1));
        let starter = Starter::Rain.effect().emitters[0].system;
        assert_eq!(first.space, ParticleSpace::World);
        assert!(first.box_size[0] > starter.box_size[0]);
        assert!(first.rate * LAYERS as f32 > starter.rate, "as dense over the wider box");
        assert_ne!(first.seed, second.seed);
        let flake = Starter::Snow.effect().emitters[0].system.size;
        assert!(layer_system(SNOW, 0).size[0] > flake[0], "flakes big enough to see");
        assert_eq!((layers_on(0.0), layers_on(0.01), layers_on(0.5), layers_on(1.0)), (0, 1, 2, LAYERS));
    }
}
