//! The upscalers on a real GPU, rendering offscreen: AMD FSR 3.1 against
//! Voxear's own, from the scene drawn at a part of the picture's size. Each
//! test skips itself (with a note) on machines without a usable Vulkan
//! device, or, for FSR, without one that can run it.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MeshRenderer, PostProcess, Primitive, Renderer,
    RendererSettings, UpscaleQuality, Upscaler,
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

/// Keeps a picture for looking at close up, where `VOXEAR_TEST_OUTPUT` names a folder.
fn keep(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, SIZE.0, SIZE.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// Three coloured cubes on a floor in the sun, after enough frames for the
/// upscaler to settle, and the frame after that.
fn cubes(renderer: &mut Renderer, upscaler: Upscaler, quality: UpscaleQuality) -> (Vec<u8>, Vec<u8>) {
    let anti_aliasing = AntiAliasing {
        mode: AntiAliasingMode::Taa,
        upscaler,
        quality: Some(quality),
        sharpness: 0.0,
        ..AntiAliasing::default()
    };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.6, 0.0), Quat::from_rotation_x(0.3), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    for (x, colour) in [(-1.5, [0.9, 0.2, 0.2, 1.0]), (0.0, [0.2, 0.8, 0.3, 1.0]), (1.5, [0.2, 0.3, 0.9, 1.0])] {
        let material = renderer.add_material(&MaterialDesc { base_color: colour, ..MaterialDesc::default() }).unwrap();
        let place = Transform::new(Vec3::new(x, 0.5, 5.0), Quat::from_rotation_y(0.4), Vec3::ONE);
        world.spawn((MeshRenderer::new(cube, material), place));
    }
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 2.5), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(post);
    let mut time = 0.0;
    for _ in 0..40 {
        time += FRAME;
        renderer.render(&world, time).expect("frame renders");
    }
    let settled = renderer.read_pixels().unwrap().2;
    renderer.render(&world, time + FRAME).expect("frame renders");
    (settled, renderer.read_pixels().unwrap().2)
}

/// The average difference per channel between two pictures.
fn apart(a: &[u8], b: &[u8]) -> f32 {
    let total: i64 = a.iter().zip(b).map(|(a, b)| (i64::from(*a) - i64::from(*b)).abs()).sum();
    total as f32 / a.len() as f32
}

#[test]
fn fsr_upscales_a_scene_drawn_at_half_size_close_to_the_full_one_and_holds_still() {
    let Some(mut renderer) = renderer() else { return };
    if !renderer.upscalers().iter().any(|(upscaler, why)| *upscaler == Upscaler::Fsr && why.is_none()) {
        return eprintln!("skipping: this GPU cannot run FSR");
    }
    let (full, _) = cubes(&mut renderer, Upscaler::Voxear, UpscaleQuality::Native);
    let (voxear, _) = cubes(&mut renderer, Upscaler::Voxear, UpscaleQuality::Performance);
    let (fsr, next) = cubes(&mut renderer, Upscaler::Fsr, UpscaleQuality::Performance);
    keep("upscale full size.png", &full);
    keep("upscale voxear half.png", &voxear);
    keep("upscale fsr half.png", &fsr);
    let (from_full, from_voxear, still) = (apart(&full, &fsr), apart(&voxear, &fsr), apart(&fsr, &next));
    eprintln!(
        "FSR from half size: {from_full:.2} from full size, {from_voxear:.2} from Voxear's, {still:.2} from the next frame"
    );
    assert!(apart(&full, &voxear) < 2.0, "Voxear's own, as its own test has it");
    assert!(from_full < 3.0, "FSR's picture is the scene's: {from_full} apart on average");
    assert!(still < 1.0, "and holds still from frame to frame: {still}");
}

#[test]
fn upscalers_say_whether_they_run_here() {
    let Some(renderer) = renderer() else { return };
    let upscalers = renderer.upscalers();
    assert_eq!(upscalers[0], (Upscaler::Voxear, None), "Voxear's own runs everywhere");
    let why = |wanted: Upscaler| upscalers.iter().find(|(upscaler, _)| *upscaler == wanted).unwrap().1.clone();
    assert!(why(Upscaler::Dlss).is_some_and(|why| why.contains("DLSS")), "{:?}", why(Upscaler::Dlss));
    assert!(why(Upscaler::Xess).is_some_and(|why| why.contains("XeSS")), "{:?}", why(Upscaler::Xess));
}
