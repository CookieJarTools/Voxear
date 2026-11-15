//! Sound rays on a real GPU, by its ray queries and in software: a wall
//! between the listener and a source stops its rays and lets through what
//! its acoustic material does; a source in the open is heard whole; and a
//! closed room sends back far more of a sound, and for far longer, than
//! open ground does. Each test skips itself (with a note) on machines
//! without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    Camera, MaterialDesc, MeshRenderer, Primitive, Renderer, RendererSettings, SOUND_FIELD, SOUND_FIXED, SoundHeard,
    SoundMaterial, SoundRays, SoundSource,
};
use voxear_scene::Transform;

/// Steam Audio's concrete, the third of the presets as the engine numbers them.
const CONCRETE: SoundMaterial =
    SoundMaterial { absorption: [0.05, 0.07, 0.08], scattering: 0.05, transmission: [0.015, 0.002, 0.001] };

fn renderer(software: bool) -> Option<Renderer> {
    match Renderer::new_offscreen(64, 64, &RendererSettings { software_rays: software, ..Default::default() }) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// A world of concrete slabs, each a middle and a size, seen by a camera.
fn slabs(renderer: &mut Renderer, slabs: &[(Vec3, Vec3)]) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.0, -2.0), Quat::IDENTITY, Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let concrete = renderer.add_material(&MaterialDesc { acoustic: 3, ..MaterialDesc::default() }).unwrap();
    for &(middle, size) in slabs {
        world.spawn((MeshRenderer::new(cube, concrete), Transform::new(middle, Quat::IDENTITY, size)));
    }
    world
}

/// Traces `asked` until what it found comes back.
fn trace(renderer: &mut Renderer, world: &World, asked: &SoundRays) -> SoundHeard {
    let mut time = 0.0;
    for _ in 0..12 {
        renderer.trace_sound(Some(asked.clone()));
        time += 1.0 / 60.0;
        renderer.render(world, time).expect("frame renders");
        if let Some(heard) = renderer.sound_heard().filter(|heard| heard.id == asked.id) {
            return heard;
        }
    }
    panic!("the sound rays came back");
}

fn materials() -> Vec<SoundMaterial> {
    let generic = SoundMaterial { absorption: [0.1, 0.2, 0.3], scattering: 0.05, transmission: [0.1, 0.05, 0.03] };
    vec![generic, generic, CONCRETE]
}

fn a_wall_stops_a_source_behind_it_and_lets_through_what_concrete_does(software: bool) {
    let Some(mut renderer) = renderer(software) else { return };
    // A wall across the X axis at x = 3, 10 metres square and 20 cm thick.
    let world = slabs(&mut renderer, &[(Vec3::new(3.0, 0.0, 0.0), Vec3::new(0.2, 10.0, 10.0))]);
    let source = |at: Vec3, rays: u32| SoundSource { at, radius: 0.5, rays };
    let asked = SoundRays {
        id: 1,
        listener: Vec3::ZERO,
        sources: vec![
            source(Vec3::new(6.0, 0.0, 0.0), 1),
            source(Vec3::new(-6.0, 0.0, 0.0), 1),
            source(Vec3::new(6.0, 0.0, 0.0), 16),
        ],
        materials: materials(),
        ..SoundRays::default()
    };
    let heard = trace(&mut renderer, &world, &asked);
    let (open, through) = heard.sources[0];
    assert_eq!(open, 0, "behind the wall");
    for (band, &t) in through.iter().enumerate() {
        assert!((t - CONCRETE.transmission[band]).abs() < 1e-3, "through concrete in band {band}: {through:?}");
    }
    assert_eq!(heard.sources[1], (1, [0.0; 3]), "in the open");
    assert_eq!(heard.sources[2].0, 0, "a sphere wholly behind it");
}

#[test]
fn a_wall_stops_a_source_by_ray_queries() {
    a_wall_stops_a_source_behind_it_and_lets_through_what_concrete_does(false);
}

#[test]
fn a_wall_stops_a_source_in_software() {
    a_wall_stops_a_source_behind_it_and_lets_through_what_concrete_does(true);
}

/// The energy all round in the middle band, from `from` seconds on.
fn late(field: &[i32], from: f32) -> f32 {
    let bins = SOUND_FIELD / 12;
    let middle = &field[bins..2 * bins];
    middle.iter().skip((from / 0.01) as usize).map(|&n| n as f32 / SOUND_FIXED).sum()
}

fn a_closed_room_rings_and_open_ground_does_not(software: bool) {
    let Some(mut renderer) = renderer(software) else { return };
    let room = |half: f32| {
        let t = 0.2;
        vec![
            (Vec3::new(0.0, -half, 0.0), Vec3::new(2.0 * half, t, 2.0 * half)),
            (Vec3::new(0.0, half, 0.0), Vec3::new(2.0 * half, t, 2.0 * half)),
            (Vec3::new(-half, 0.0, 0.0), Vec3::new(t, 2.0 * half, 2.0 * half)),
            (Vec3::new(half, 0.0, 0.0), Vec3::new(t, 2.0 * half, 2.0 * half)),
            (Vec3::new(0.0, 0.0, -half), Vec3::new(2.0 * half, 2.0 * half, t)),
            (Vec3::new(0.0, 0.0, half), Vec3::new(2.0 * half, 2.0 * half, t)),
        ]
    };
    let asked = SoundRays {
        id: 2,
        listener: Vec3::ZERO,
        spots: vec![Vec3::ZERO],
        rays: 2048,
        bounces: 16,
        seconds: 2.0,
        materials: materials(),
        ..SoundRays::default()
    };
    let closed = slabs(&mut renderer, &room(5.0));
    let closed = trace(&mut renderer, &closed, &asked);
    let ground = slabs(&mut renderer, &[(Vec3::new(0.0, -5.0, 0.0), Vec3::new(40.0, 0.2, 40.0))]);
    let open = trace(&mut renderer, &ground, &SoundRays { id: 3, ..asked.clone() });
    let (closed, open) = (&closed.fields[0], &open.fields[0]);
    let (early_closed, early_open) = (late(closed, 0.0), late(open, 0.0));
    assert!(early_open > 0.0, "the ground answers once");
    // Steam Audio's energies are small: a sound's energy a metre off is 1/4π, spread over every ray.
    let (late_closed, late_open) = (late(closed, 0.3), late(open, 0.3));
    assert!(late_closed > 1e-4 && late_open == 0.0, "after 300 ms: closed {late_closed} open {late_open}");
    assert!(early_closed > early_open * 5.0, "closed {early_closed} open {early_open}");
}

#[test]
fn a_closed_room_rings_by_ray_queries() {
    a_closed_room_rings_and_open_ground_does_not(false);
}

#[test]
fn a_closed_room_rings_in_software() {
    a_closed_room_rings_and_open_ground_does_not(true);
}

/// The pass's GPU time at each quality, the least of 60 frames, by ray queries and in software, in a closed room
/// of 20 m with 200 boxes in it and 16 sources, half of them volumetric. Run with `--ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn sound_rays_cost() {
    for software in [false, true] {
        let Some(mut renderer) = renderer(software) else { return };
        let mut parts: Vec<(Vec3, Vec3)> = Vec::new();
        let (h, t, s) = (10.0, 0.2, 20.0);
        for (at, size) in [
            ([0.0, -h, 0.0], [s, t, s]),
            ([0.0, h, 0.0], [s, t, s]),
            ([-h, 0.0, 0.0], [t, s, s]),
            ([h, 0.0, 0.0], [t, s, s]),
            ([0.0, 0.0, -h], [s, s, t]),
            ([0.0, 0.0, h], [s, s, t]),
        ] {
            parts.push((Vec3::from(at), Vec3::from(size)));
        }
        for i in 0..200 {
            let f = i as f32;
            let at = Vec3::new((f * 7.3) % 18.0 - 9.0, (f * 3.1) % 18.0 - 9.0, (f * 5.7) % 18.0 - 9.0);
            parts.push((at, Vec3::splat(0.5 + (f * 0.37) % 1.0)));
        }
        let world = slabs(&mut renderer, &parts);
        let sources: Vec<SoundSource> = (0..16)
            .map(|i| SoundSource {
                at: Vec3::new(i as f32 - 8.0, 0.0, 6.0),
                radius: 0.5,
                rays: if i % 2 == 0 { 1 } else { 8 },
            })
            .collect();
        for (quality, rays, bounces) in [("low", 256, 6), ("medium", 1024, 10), ("high", 4096, 16)] {
            let mut least = f32::MAX;
            let mut time = 0.0;
            for frame in 0..80u64 {
                let asked = SoundRays {
                    id: frame + 1,
                    listener: Vec3::new(0.5, 1.0, -2.0),
                    sources: sources.clone(),
                    spots: vec![Vec3::new(0.5, 1.0, -2.0)],
                    rays,
                    bounces,
                    seconds: 2.0,
                    materials: materials(),
                };
                renderer.trace_sound(Some(asked));
                time += 1.0 / 60.0;
                renderer.render(&world, time).unwrap();
                if frame >= 20
                    && let Some(&(_, ms)) = renderer.timings().iter().find(|(name, _)| name == "Sound rays")
                {
                    least = least.min(ms);
                }
            }
            let by = if software { "software" } else { "ray queries" };
            println!("sound rays, {by}, {quality} ({rays} rays, {bounces} bounces, 16 sources): {least:.3} ms");
        }
    }
}
