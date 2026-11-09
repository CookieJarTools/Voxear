//! Plants drawn on a real GPU, offscreen: a plant taken away is not drawn,
//! and the rest are. Skips itself on machines without a usable Vulkan
//! device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Foliage, Impostors, MaterialDesc, Plant, Plants, PostProcess, Primitive,
    Renderer, RendererSettings, ViewOptions,
};
use voxear_scene::Transform;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;

#[test]
fn a_plant_taken_away_is_not_drawn_and_the_rest_are() {
    let Ok(mut renderer) = Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let mut world = World::new();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let red =
        renderer.add_material(&MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ..MaterialDesc::default() }).unwrap();
    let foliage = Foliage {
        mesh: cube,
        material: red,
        draw_distance: 100.0,
        fade_distance: 1.0,
        shadow_distance: 0.0,
        healthy_color: [1.0; 3],
        dry_color: [1.0; 3],
        color_spread: 8.0,
        impostors: Impostors::Off,
        impostor_distance: 60.0,
    };
    // Two cubes 2 m across side by side, 10 m ahead of the camera.
    let plants = Plants::new(vec![Plant::new([-2.0, 0.0, 0.0], 0.0, 2.0), Plant::new([2.0, 0.0, 0.0], 0.0, 2.0)]);
    let grove = world.spawn((foliage, plants, Transform::from_position(Vec3::new(0.0, 0.0, 10.0))));
    world.spawn((Camera::default(), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let red_at = |renderer: &mut Renderer, world: &World, x: u32| {
        for frame in 0..3 {
            renderer.render(world, frame as f32 / 60.0).unwrap();
        }
        let (_, _, pixels) = renderer.read_pixels().unwrap();
        let i = ((HEIGHT / 2 * WIDTH + x) * 4) as usize;
        pixels[i] > 150 && pixels[i + 1] < 60
    };
    // 2 m either side at 10 m is about a fifth of the way to the screen's side; one cube left, one right.
    let (left, right) = (WIDTH / 2 - 31, WIDTH / 2 + 31);
    assert!(red_at(&mut renderer, &world, left) && red_at(&mut renderer, &world, right), "both drawn");
    world.get_mut::<Plants>(grove).unwrap().remove(1);
    let (left_after, right_after) = (red_at(&mut renderer, &world, left), red_at(&mut renderer, &world, right));
    assert!(left_after != right_after, "one taken away, the other still drawn: {left_after} {right_after}");
}
