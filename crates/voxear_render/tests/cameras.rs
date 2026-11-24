//! A Camera's settings past its lens, on a real GPU, rendering offscreen:
//! viewports side by side, a camera stacked over another, a background of
//! one colour, the layers it sees, a far limit, an oblique clip plane, HDR
//! off and a picture shown on a screen. Each test skips itself (with a note)
//! on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Background, Camera, CameraOver, CameraScreen, CameraTarget, Light, MaterialDesc,
    MeshRenderer, PostProcess, Primitive, Renderer, RendererSettings, ViewOptions,
};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;
const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// The frames drawn through the world's cameras, as the game draws them.
fn frames(renderer: &mut Renderer, world: &World, count: u32) -> Vec<u8> {
    let mut time = 0.0;
    for _ in 0..count {
        time += FRAME;
        renderer.look_through_cameras(world, ViewOptions::default()).unwrap();
        renderer.render(world, time).expect("frame renders");
    }
    renderer.read_pixels().unwrap().2
}

fn at(pixels: &[u8], (x, y): (u32, u32)) -> [i32; 3] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from)
}

/// A world with a sun, plain post-processing and a cube of each colour,
/// none of it a camera yet.
struct Stage {
    world: World,
    cube: voxear_render::MeshHandle,
}

impl Stage {
    fn new(renderer: &mut Renderer) -> Self {
        let mut world = World::new();
        let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
        world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
        let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
        world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
        let cube = renderer.primitive(Primitive::Cube).unwrap();
        Self { world, cube }
    }

    fn cube(&mut self, renderer: &mut Renderer, rgb: [f32; 3], place: Transform) -> Entity {
        let desc = MaterialDesc { base_color: [rgb[0], rgb[1], rgb[2], 1.0], ..MaterialDesc::default() };
        let material = renderer.add_material(&desc).unwrap();
        self.world.spawn((MeshRenderer::new(self.cube, material), place))
    }

    fn camera(&mut self, camera: Camera, at: Vec3) -> Entity {
        self.world.spawn((camera, Transform::from_position(at)))
    }
}

fn reddish(rgb: [i32; 3]) -> bool {
    rgb[0] > 60 && rgb[0] > rgb[1] * 2 && rgb[0] > rgb[2] * 2
}

fn greenish(rgb: [i32; 3]) -> bool {
    rgb[1] > 60 && rgb[1] > rgb[0] * 2 && rgb[1] > rgb[2] * 2
}

#[test]
fn two_cameras_split_the_screen_by_their_viewports() {
    let Some(mut renderer) = renderer() else { return };
    let mut stage = Stage::new(&mut renderer);
    stage.cube(&mut renderer, [0.9, 0.05, 0.05], Transform::from_position(Vec3::new(0.0, 0.0, 3.0)));
    stage.cube(&mut renderer, [0.05, 0.9, 0.05], Transform::from_position(Vec3::new(100.0, 0.0, 3.0)));
    stage.camera(Camera { viewport: [0.0, 0.0, 0.5, 1.0], ..Camera::default() }, Vec3::ZERO);
    stage.camera(Camera { viewport: [0.5, 0.0, 0.5, 1.0], ..Camera::default() }, Vec3::new(100.0, 0.0, 0.0));
    let picture = frames(&mut renderer, &stage.world, 4);
    keep_image("split screen.png", &picture);
    assert!(reddish(at(&picture, (160, 180))), "the left half sees the red cube: {:?}", at(&picture, (160, 180)));
    assert!(greenish(at(&picture, (480, 180))), "the right half the green one: {:?}", at(&picture, (480, 180)));
}

#[test]
fn a_camera_stacked_over_another_covers_it_only_where_it_draws() {
    let Some(mut renderer) = renderer() else { return };
    let mut stage = Stage::new(&mut renderer);
    // The world, a red wall filling the view; and far off, a small green cube only the stacked camera sees.
    stage.cube(&mut renderer, [0.9, 0.05, 0.05], Transform::new(Vec3::new(0.0, 0.0, 6.0), Quat::IDENTITY, Vec3::new(20.0, 20.0, 1.0)));
    let hands = stage.cube(&mut renderer, [0.05, 0.9, 0.05], Transform::from_position(Vec3::new(500.0, 0.0, 3.0)));
    stage.world.insert(hands, voxear_scene::Layer(5));
    let world_camera = Camera { layers: !(1 << 5), ..Camera::default() };
    let base = stage.camera(world_camera, Vec3::ZERO);
    let over = Camera { layers: 1 << 5, background: Background::Nothing, priority: 1, ..Camera::default() };
    let over = stage.camera(over, Vec3::new(500.0, 0.0, 0.0));
    stage.world.insert(over, CameraOver(base));
    let picture = frames(&mut renderer, &stage.world, 4);
    keep_image("stacked.png", &picture);
    assert!(greenish(at(&picture, (320, 180))), "the stacked camera's cube in the middle: {:?}", at(&picture, (320, 180)));
    assert!(reddish(at(&picture, (40, 40))), "the world's wall round it: {:?}", at(&picture, (40, 40)));
}

#[test]
fn a_background_of_one_colour_fills_where_nothing_is_seen() {
    let Some(mut renderer) = renderer() else { return };
    let mut stage = Stage::new(&mut renderer);
    stage.cube(&mut renderer, [0.9, 0.05, 0.05], Transform::from_position(Vec3::new(0.0, 0.0, 3.0)));
    let camera = Camera { background: Background::Color { color: [0.1, 0.3, 0.9] }, ..Camera::default() };
    stage.camera(camera, Vec3::ZERO);
    let picture = frames(&mut renderer, &stage.world, 4);
    keep_image("background colour.png", &picture);
    let sky = at(&picture, (20, 20));
    assert!(sky[2] > sky[0] + 60 && sky[2] > sky[1] + 30, "blue where the sky was: {sky:?}");
    assert!(reddish(at(&picture, (320, 180))), "the cube drawn over it");
}

#[test]
fn a_camera_sees_only_its_layers_and_nothing_past_its_far_limit() {
    let Some(mut renderer) = renderer() else { return };
    let mut stage = Stage::new(&mut renderer);
    let hidden = stage.cube(&mut renderer, [0.9, 0.05, 0.05], Transform::from_position(Vec3::new(-1.0, 0.0, 3.0)));
    stage.world.insert(hidden, voxear_scene::Layer(3));
    stage.cube(&mut renderer, [0.05, 0.9, 0.05], Transform::from_position(Vec3::new(1.0, 0.0, 3.0)));
    stage.cube(&mut renderer, [0.05, 0.9, 0.05], Transform::from_position(Vec3::new(0.0, 1.5, 20.0)));
    let camera = Camera { layers: !(1 << 3), far: Some(10.0), ..Camera::default() };
    stage.camera(camera, Vec3::ZERO);
    let picture = frames(&mut renderer, &stage.world, 4);
    keep_image("layers and far.png", &picture);
    let left = on_screen(Vec3::new(-1.0, 0.0, 3.0));
    let right = on_screen(Vec3::new(1.0, 0.0, 3.0));
    let far = on_screen(Vec3::new(0.0, 1.5, 20.0));
    assert!(!reddish(at(&picture, left)), "layer 3's cube is left out: {:?}", at(&picture, left));
    assert!(greenish(at(&picture, right)), "the default layer's is drawn: {:?}", at(&picture, right));
    assert!(!greenish(at(&picture, far)), "the cube 20 m off is past the far limit: {:?}", at(&picture, far));
}

#[test]
fn an_oblique_clip_plane_cuts_away_what_is_in_front_of_it() {
    let Some(mut renderer) = renderer() else { return };
    let mut stage = Stage::new(&mut renderer);
    stage.cube(&mut renderer, [0.9, 0.05, 0.05], Transform::from_position(Vec3::new(0.0, 0.0, 3.0)));
    stage.cube(&mut renderer, [0.05, 0.9, 0.05], Transform::new(Vec3::new(0.0, 0.0, 8.0), Quat::IDENTITY, Vec3::splat(4.0)));
    // Keep only what is 5 m or more ahead: the red cube in front goes, the green one behind shows.
    let camera = Camera { clip_plane: Some([0.0, 0.0, 1.0, -5.0]), ..Camera::default() };
    stage.camera(camera, Vec3::ZERO);
    let picture = frames(&mut renderer, &stage.world, 4);
    keep_image("clip plane.png", &picture);
    assert!(greenish(at(&picture, (320, 180))), "the far cube shows through: {:?}", at(&picture, (320, 180)));
}

#[test]
fn without_hdr_light_is_cut_at_white() {
    let shot = |hdr: bool| -> Option<i32> {
        let mut renderer = renderer()?;
        let mut stage = Stage::new(&mut renderer);
        let desc = MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [40.0, 40.0, 40.0], ..MaterialDesc::default() };
        let glow = renderer.add_material(&desc).unwrap();
        stage.world.spawn((MeshRenderer::new(stage.cube, glow), Transform::from_position(Vec3::new(0.0, 0.0, 3.0))));
        let bloom = voxear_render::Bloom { enabled: true, intensity: 0.5, ..voxear_render::Bloom::default() };
        let post = PostProcess { bloom, ..*stage.world.resource::<PostProcess>().unwrap() };
        stage.world.insert_resource(post);
        let finish = voxear_render::CameraFinish { hdr, ..voxear_render::CameraFinish::default() };
        stage.camera(Camera { finish, ..Camera::default() }, Vec3::ZERO);
        let picture = frames(&mut renderer, &stage.world, 4);
        // The glow round the cube, from what is brighter than white.
        Some(at(&picture, (320, 60)).iter().sum())
    };
    let (Some(hdr), Some(ldr)) = (shot(true), shot(false)) else { return };
    assert!(hdr > ldr + 30, "light past white glows with HDR, and is cut without it: {hdr} against {ldr}");
}

#[test]
fn a_cameras_picture_shows_on_a_screen() {
    let Some(mut renderer) = renderer() else { return };
    let mut stage = Stage::new(&mut renderer);
    // Far off, a green cube a security camera sees; in front of the main camera, a screen showing its picture.
    stage.cube(&mut renderer, [0.05, 0.9, 0.05], Transform::from_position(Vec3::new(300.0, 0.0, 3.0)));
    let watcher = Camera { target: CameraTarget::Picture { width: 128, height: 128 }, ..Camera::default() };
    let watcher = stage.camera(watcher, Vec3::new(300.0, 0.0, 0.0));
    let screen = stage.cube(&mut renderer, [0.5, 0.5, 0.5], Transform::new(Vec3::new(0.0, 0.0, 3.0), Quat::IDENTITY, Vec3::new(3.0, 3.0, 0.1)));
    stage.world.insert(screen, CameraScreen { camera: watcher, brightness: 1.0 });
    stage.camera(Camera::default(), Vec3::ZERO);
    let mut time = 0.0;
    for _ in 0..4 {
        time += FRAME;
        renderer.look_through_cameras(&stage.world, ViewOptions::default()).unwrap();
        voxear_render_screens(&mut stage.world, &mut renderer);
        renderer.render(&stage.world, time).expect("frame renders");
    }
    assert!(renderer.camera_picture(watcher).is_some(), "the camera has a picture");
    let picture = renderer.read_pixels().unwrap().2;
    keep_image("screen.png", &picture);
    // Through the main camera's own exposure and tonemap the green is paler than the cube's, but the grey screen
    // turns clearly green.
    let [r, g, b] = at(&picture, (320, 180));
    assert!(g > r + 40 && g > b + 40, "the screen shows the green cube: {:?}", [r, g, b]);
}

/// What `voxear_engine::cameras::show_screens` does, which the render crate
/// cannot call: the screen's material, glowing with the camera's picture.
fn voxear_render_screens(world: &mut World, renderer: &mut Renderer) {
    let screens: Vec<(Entity, CameraScreen)> = world.query_ref::<(Entity, &CameraScreen)>().into_iter().map(|(e, s)| (e, *s)).collect();
    for (entity, screen) in screens {
        let Some(picture) = renderer.camera_picture(screen.camera) else { continue };
        let desc = MaterialDesc {
            base_color: [0.0, 0.0, 0.0, 1.0],
            emissive: [screen.brightness; 3],
            emissive_texture: Some(picture),
            ..MaterialDesc::default()
        };
        let material = renderer.add_material(&desc).unwrap();
        world.get_mut::<MeshRenderer>(entity).unwrap().material = material;
    }
}

/// Where `point` is on screen from a camera at the origin looking along +Z,
/// as the default camera's 60° projection puts it.
fn on_screen(point: Vec3) -> (u32, u32) {
    use voxear_core::math::camera::lh::proj::vulkan;
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), WIDTH as f32 / HEIGHT as f32, 0.1) * point.extend(1.0);
    (((clip.x / clip.w * 0.5 + 0.5) * WIDTH as f32) as u32, ((clip.y / clip.w * 0.5 + 0.5) * HEIGHT as f32) as u32)
}
