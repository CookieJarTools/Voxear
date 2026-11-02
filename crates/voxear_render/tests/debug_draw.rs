//! Debug draw on a real GPU, rendering offscreen. A file of its own, since
//! debug draw keeps one list for the process and other tests' frames must
//! not show these lines. Skips itself without a usable Vulkan device.

use voxear_core::{
    debug_draw,
    math::{Quat, Vec3},
};
use voxear_ecs::World;
use voxear_render::{Camera, MaterialDesc, MeshRenderer, Primitive, Renderer, RendererSettings, ViewOptions};
use voxear_scene::Transform;

const SIZE: (u32, u32) = (640, 360);

/// A camera looking down +Z at a wall that fills the left half of the view.
fn wall(renderer: &mut Renderer) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -10.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((
        MeshRenderer { mesh: cube, material },
        Transform::new(Vec3::new(-6.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(12.0, 12.0, 0.5)),
    ));
    world
}

/// A few frames rendered, as TAA settles, and the last read back.
fn frame(renderer: &mut Renderer, world: &World, time: &mut f32) -> Vec<u8> {
    for _ in 0..4 {
        *time += 1.0 / 60.0;
        renderer.render(world, *time).unwrap();
    }
    renderer.read_pixels().unwrap().2
}

/// Pixels in the left and right halves that are now mostly `channel` where
/// they were not before.
fn changed(before: &[u8], after: &[u8], channel: usize) -> (usize, usize) {
    let (mut left, mut right) = (0, 0);
    for (i, (was, now)) in before.chunks(4).zip(after.chunks(4)).enumerate() {
        let strong = now[channel] > 150 && (0..3).all(|c| c == channel || now[c] < now[channel] / 2);
        if strong && was != now {
            if (i as u32 % SIZE.0) < SIZE.0 / 2 { left += 1 } else { right += 1 }
        }
    }
    (left, right)
}

/// Pixels in the left half that changed at all, however faintly.
fn touched_left(before: &[u8], after: &[u8]) -> usize {
    let pixels = before.chunks(4).zip(after.chunks(4)).enumerate();
    pixels.filter(|(i, (was, now))| (*i as u32 % SIZE.0) < SIZE.0 / 2 && was != now).count()
}

#[test]
fn lines_dim_behind_the_scene_unless_drawn_on_top() {
    let Some(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()).ok() else {
        return eprintln!("skipping: no usable GPU");
    };
    let world = wall(&mut renderer);
    let mut time = 0.0;
    debug_draw::clear();
    let plain = frame(&mut renderer, &world, &mut time);

    // Red behind the wall, dimmed; red in plain view on the right, shown in full.
    debug_draw::line(Vec3::new(-6.0, 1.0, 6.0), Vec3::new(-2.0, 1.0, 6.0), [1.0, 0.0, 0.0, 1.0], 10.0, false);
    debug_draw::line(Vec3::new(2.0, 1.0, 0.0), Vec3::new(6.0, 1.0, 0.0), [1.0, 0.0, 0.0, 1.0], 10.0, false);
    let drawn = frame(&mut renderer, &world, &mut time);
    let (left, right) = changed(&plain, &drawn, 0);
    assert_eq!(left, 0, "the wall dims the line behind it, so it is never strong there");
    assert!(touched_left(&plain, &drawn) > 50, "but it still shows faintly through");
    assert!(right > 100, "the line in plain view shows: {right} pixels");

    // A line asking for 6 pixels covers more than one of the usual width.
    debug_draw::clear();
    debug_draw::line(Vec3::new(2.0, 1.0, 0.0), Vec3::new(6.0, 1.0, 0.0), [1.0, 0.0, 0.0, 1.0], 10.0, false);
    let thin = changed(&plain, &frame(&mut renderer, &world, &mut time), 0).1;
    debug_draw::clear();
    let wide = debug_draw::LineStyle { width: 6.0, ..Default::default() };
    debug_draw::line(Vec3::new(2.0, 1.0, 0.0), Vec3::new(6.0, 1.0, 0.0), [1.0, 0.0, 0.0, 1.0], 10.0, wide);
    let thick = changed(&plain, &frame(&mut renderer, &world, &mut time), 0).1;
    assert!(thick > thin * 2, "a wider line covers more: {thick} pixels against {thin}");

    // A gizmo's line shows only in a view showing gizmos, as the editor's Game view does not.
    debug_draw::clear();
    let gizmo = debug_draw::LineStyle { gizmo: true, ..Default::default() };
    debug_draw::line(Vec3::new(2.0, 1.0, 0.0), Vec3::new(6.0, 1.0, 0.0), [1.0, 0.0, 0.0, 1.0], 10.0, gizmo);
    let shown = changed(&plain, &frame(&mut renderer, &world, &mut time), 0).1;
    renderer.set_view_options(ViewOptions { gizmos: false, ..ViewOptions::default() });
    let hidden = changed(&plain, &frame(&mut renderer, &world, &mut time), 0).1;
    renderer.set_view_options(ViewOptions::default());
    assert!(shown > 100 && hidden == 0, "a gizmo's line shows where gizmos do: {shown} pixels, then {hidden}");
    debug_draw::clear();

    // Green behind the wall, on top: it shows through.
    debug_draw::line(Vec3::new(-6.0, 2.0, 6.0), Vec3::new(-2.0, 2.0, 6.0), [0.0, 1.0, 0.0, 1.0], 10.0, true);
    let on_top = frame(&mut renderer, &world, &mut time);
    let (left, _) = changed(&plain, &on_top, 1);
    assert!(left > 50, "drawn on top, it shows over the wall: {left} pixels");

    // A sphere half behind the camera draws without stray lines across the view.
    debug_draw::clear();
    debug_draw::sphere(Vec3::new(0.0, 1.0, -10.0), 3.0, [0.0, 0.0, 1.0, 1.0], 10.0, true);
    let around = frame(&mut renderer, &world, &mut time);
    let (left, right) = changed(&plain, &around, 2);
    assert!(left + right > 0 && left + right < (SIZE.0 * SIZE.1 / 4) as usize, "{left} + {right} pixels");

    debug_draw::clear();
    let cleared = frame(&mut renderer, &world, &mut time);
    assert_eq!(changed(&plain, &cleared, 0), (0, 0), "nothing once cleared");
    if let Some(dir) = std::env::var_os("VOXEAR_TEST_OUTPUT") {
        let path = std::path::Path::new(&dir).join("debug-draw.png");
        image::save_buffer(path, &on_top, SIZE.0, SIZE.1, image::ColorType::Rgba8).unwrap();
    }
}
