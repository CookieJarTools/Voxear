//! What the lighting looks bring beyond their settings, on a real GPU,
//! rendering offscreen: the sun through a canopy of leaves, and a blur of
//! the near foreground alone. Each test skips itself (with a note) on
//! machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, DepthOfField, Light, MaterialDesc, MeshRenderer, PostProcess, Primitive,
    Renderer, RendererSettings, Sky, SunLook,
};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;
const SIZE: (u32, u32) = (640, 360);

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// Fixed exposure and nothing over the picture that moves from frame to frame.
fn plain_post() -> PostProcess {
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    post
}

fn render(renderer: &mut Renderer, world: &World) -> Vec<u8> {
    let mut time = 0.0;
    for _ in 0..4 {
        time += FRAME;
        renderer.render(world, time).expect("frame renders");
    }
    renderer.read_pixels().unwrap().2
}

fn red(pixels: &[u8], x: u32, y: u32) -> f32 {
    f32::from(pixels[((y * SIZE.0 + x) * 4) as usize])
}

/// A floor seen from above under a high sun, with `canopy` metres of
/// leaves over it, or none.
fn floor_under(renderer: &mut Renderer, canopy: f32) -> Vec<u8> {
    let mut world = World::new();
    let down = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -1.0, 0.2).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 8.0, 0.0), down, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.2, -1.0, 0.1).normalize());
    world.spawn((Light::directional(Vec3::ONE, 2.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    world.insert_resource(SunLook {
        tint: Vec3::ONE,
        strength: 1.0,
        softness: 0.5,
        shadow_tint: Vec3::ZERO,
        shadow_distance: voxear_render::SHADOW_DISTANCE,
        shadow_split: voxear_render::SHADOW_SPLIT,
        canopy,
    });
    world.insert_resource(plain_post());
    render(renderer, &world)
}

/// How much the middle row's brightness varies, as its standard deviation,
/// and its average.
fn row_spread(pixels: &[u8]) -> (f32, f32) {
    let y = SIZE.1 / 2;
    let values: Vec<f32> = (40..SIZE.0 - 40).map(|x| red(pixels, x, y)).collect();
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let spread = (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt();
    (spread, mean)
}

#[test]
fn a_canopy_dapples_the_sunlit_ground() {
    let Some(mut renderer) = renderer() else { return };
    let open = floor_under(&mut renderer, 0.0);
    let under = floor_under(&mut renderer, 6.0);
    let ((open_spread, open_mean), (under_spread, under_mean)) = (row_spread(&open), row_spread(&under));
    eprintln!(
        "open sky: spread {open_spread:.1}, mean {open_mean:.1}; canopy: spread {under_spread:.1}, mean {under_mean:.1}"
    );
    assert!(open_spread < 6.0, "open ground is even: {open_spread}");
    assert!(under_spread > 20.0, "flecks of sun and shade: {under_spread}");
    assert!(under_mean < open_mean - 20.0, "mostly shade: {under_mean} against {open_mean}");
    let brightest = (40..SIZE.0 - 40).map(|x| red(&under, x, SIZE.1 / 2)).fold(0.0f32, f32::max);
    assert!(brightest > open_mean * 0.9, "the flecks are lit as the open ground is: {brightest} against {open_mean}");
}

/// A sharp black and white edge far off, at the middle of the picture, and a
/// small white block close in front of the camera to its right, both
/// glowing in the dark, under depth of field focused at 3 m, blurring the
/// far side too or not; or with none.
fn edges(renderer: &mut Renderer, far: Option<bool>) -> Vec<u8> {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::ZERO)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let glow = |renderer: &mut Renderer, light: f32| {
        let material =
            MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [light; 3], ..MaterialDesc::default() };
        renderer.add_material(&material).unwrap()
    };
    let (white, black) = (glow(renderer, 1.0), glow(renderer, 0.0));
    let half = |x: f32| Transform::new(Vec3::new(x, 0.0, 20.0), Quat::IDENTITY, Vec3::new(20.0, 20.0, 1.0));
    world.spawn((MeshRenderer::new(cube, white), half(-10.0)));
    world.spawn((MeshRenderer::new(cube, black), half(10.0)));
    world.spawn((
        MeshRenderer::new(cube, white),
        Transform::new(Vec3::new(0.6, 0.0, 1.5), Quat::IDENTITY, Vec3::splat(0.3)),
    ));
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    let mut post = plain_post();
    post.fog.enabled = false;
    if let Some(far) = far {
        post.depth_of_field =
            DepthOfField { enabled: true, focus_distance: 3.0, aperture: 2.0, far, ..DepthOfField::default() };
    }
    world.insert_resource(post);
    render(renderer, &world)
}

/// The steepest step between neighbouring pixels of the middle row, from
/// `from` to `to`: high for a sharp edge, low for a blurred one.
fn steepest(pixels: &[u8], from: u32, to: u32) -> f32 {
    let y = SIZE.1 / 2;
    (from..to).map(|x| (red(pixels, x + 1, y) - red(pixels, x, y)).abs()).fold(0.0, f32::max)
}

#[test]
fn depth_of_field_can_blur_the_near_foreground_alone() {
    let Some(mut renderer) = renderer() else { return };
    let (both, near) = (edges(&mut renderer, Some(true)), edges(&mut renderer, Some(false)));
    // The far edge lies at the middle; the near block's left edge a third of the way to the right side.
    let (far_both, far_near) = (steepest(&both, 300, 340), steepest(&near, 300, 340));
    // The near block's light spreads over the dark beyond it, as a soft halo some pixels wide.
    let halo = |pixels: &[u8]| (360..500).filter(|&x| (20.0..200.0).contains(&red(pixels, x, SIZE.1 / 2))).count();
    let sharp = edges(&mut renderer, None);
    eprintln!(
        "far edge: {far_both} blurring both ways, {far_near} the near alone; the near block's halo {} pixels, {} unblurred",
        halo(&near),
        halo(&sharp)
    );
    assert!(far_near > 120.0, "the distance stays sharp: {far_near}");
    assert!(far_both < far_near * 0.5, "blurring both ways blurs it: {far_both}");
    assert!(halo(&near) >= halo(&sharp) + 6, "the near block blurs: {} against {}", halo(&near), halo(&sharp));
}
