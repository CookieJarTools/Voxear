//! End-to-end checks on a real GPU, rendering offscreen. Each test skips
//! itself (with a note) on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    Camera, Light, MaterialDesc, MeshRenderer, Primitive, RenderStats, Renderer, RendererSettings, ViewOptions,
};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(640, 360, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// Renders `frames` frames and returns the stats of the last one. Stats come
/// back a couple of frames late, so a few extra frames are rendered first.
fn run(renderer: &mut Renderer, world: &World, time: &mut f32, frames: u32) -> RenderStats {
    for _ in 0..frames + 2 {
        *time += FRAME;
        renderer.render(world, *time).expect("frame renders");
    }
    renderer.stats()
}

/// A camera looking down +Z at a wall, with a row of spheres hidden behind
/// it and another row in plain view.
fn wall_scene(renderer: &mut Renderer) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -10.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((
        MeshRenderer { mesh: cube, material },
        Transform::new(Vec3::new(-4.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 6.0, 0.5)),
    ));
    for i in 0..5 {
        let hidden = Vec3::new(-6.0 + i as f32, 1.0, 6.0);
        let visible = Vec3::new(2.0 + i as f32, 1.0, 6.0);
        world.spawn((MeshRenderer { mesh: sphere, material }, Transform::from_position(hidden)));
        world.spawn((MeshRenderer { mesh: sphere, material }, Transform::from_position(visible)));
    }
    world
}

#[test]
fn the_wall_hides_what_is_behind_it() {
    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let mut time = 0.0;
    let stats = run(&mut renderer, &world, &mut time, 4);
    assert_eq!(stats.instances, 11);
    assert_eq!(stats.occlusion_culled, 5, "{stats:?}");
    assert_eq!(stats.drawn_early + stats.drawn_late, 6, "{stats:?}");
}

#[test]
fn a_lamp_only_reaches_the_clusters_around_it() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = wall_scene(&mut renderer);
    world.spawn((Light::point(Vec3::ONE, 5.0, 2.0), Transform::from_position(Vec3::new(4.0, 1.0, 4.0))));
    let mut time = 0.0;
    let stats = run(&mut renderer, &world, &mut time, 2);
    assert_eq!(stats.cluster_max_lights, 1);
    assert!(stats.cluster_light_links > 0, "{stats:?}");
    // A 2 m light 14 m away should touch a small part of the 3,456-cluster grid.
    assert!(stats.cluster_light_links < 200, "{stats:?}");
}

#[test]
fn lod_changes_fade_then_settle() {
    let Some(mut renderer) = renderer() else { return };
    // One large sphere close up, so it starts at its most detailed LOD.
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -4.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world
        .spawn((MeshRenderer { mesh: sphere, material }, Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0))));
    let mut time = 0.0;
    let settled = run(&mut renderer, &world, &mut time, 4);
    assert_eq!(settled.fade_draws, 0, "first appearances snap: {settled:?}");

    // A much looser error budget moves the sphere to a coarser LOD.
    renderer.set_view_options(ViewOptions { lod_threshold: 16.0, ..ViewOptions::default() });
    let fading = run(&mut renderer, &world, &mut time, 1);
    assert!(fading.fade_draws > 0, "{fading:?}");

    // Fades take a quarter of a second.
    let done = run(&mut renderer, &world, &mut time, 30);
    assert_eq!(done.fade_draws, 0, "{done:?}");
}
