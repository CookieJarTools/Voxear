//! The engine's built-in shaders, compiled into the renderer's and drawn on
//! a ball on a real GPU, each with its settings as they start. Skips itself
//! on machines without a usable Vulkan device. With `VOXEAR_TEST_OUTPUT`
//! set, each picture is kept there, to look at.

use std::collections::BTreeSet;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_engine::{
    default_on_gpu,
    shaders::{BuiltinShader, ShaderRef, ShaderRegistry, dispatch_code},
};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, CustomShaderState, Light, MaterialDesc, MeshData, MeshRenderer,
    PostProcess, Primitive, Renderer, RendererSettings, Sky, Vertex,
};
use voxear_scene::Transform;

const SIZE: (u32, u32) = (640, 360);

/// On the ball, left of its middle, where the sun lights it.
const LIT: (u32, u32) = (280, 170);

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, SIZE.0, SIZE.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// The average colour of the square `half` pixels round `centre`.
fn patch(pixels: &[u8], centre: (u32, u32), half: u32) -> [i32; 3] {
    let mut total = [0; 3];
    let mut count = 0;
    for y in centre.1 - half..=centre.1 + half {
        for x in centre.0 - half..=centre.0 + half {
            let i = ((y * SIZE.0 + x) * 4) as usize;
            for c in 0..3 {
                total[c] += i32::from(pixels[i + c]);
            }
            count += 1;
        }
    }
    total.map(|sum| sum / count)
}

/// A grey ball over a grey floor in the sun, drawn with `shader` (none for
/// the standard one) at its default settings, at `time`; the floor too for
/// water and lava, which are most often floors.
fn ball(renderer: &mut Renderer, registry: &ShaderRegistry, shader: Option<BuiltinShader>, time: f32) -> Vec<u8> {
    ball_of(renderer, registry, shader, time, false)
}

/// [`ball`], the ball's material glass when `glass`.
fn ball_of(
    renderer: &mut Renderer,
    registry: &ShaderRegistry,
    shader: Option<BuiltinShader>,
    time: f32,
    glass: bool,
) -> Vec<u8> {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -2.4))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let mut desc = MaterialDesc { base_color: [0.5, 0.5, 0.5, 1.0], glass, ..MaterialDesc::default() };
    if let Some((number, info)) = shader.and_then(|shader| registry.get(ShaderRef::Builtin(shader))) {
        desc.shader = number;
        desc.max_offset = info.max_offset;
        for (slot, property) in desc.properties.iter_mut().zip(&info.properties) {
            *slot = default_on_gpu(property);
        }
    }
    let material = renderer.add_material(&desc).unwrap();
    let grey = MaterialDesc { base_color: [0.3, 0.3, 0.3, 1.0], ..MaterialDesc::default() };
    let floored = !glass && matches!(shader, Some(BuiltinShader::Water | BuiltinShader::Lava));
    let floor = if floored { material } else { renderer.add_material(&grey).unwrap() };
    world.spawn((MeshRenderer::new(plane, floor), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let at = Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::ONE);
    world.spawn((MeshRenderer::new(sphere, material), at));
    // From the left, so the ball's right side is in shade.
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(1.0, -0.5, 0.3).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    for _ in 0..6 {
        renderer.render(&world, time).unwrap();
    }
    renderer.read_pixels().unwrap().2
}

#[test]
fn every_built_in_shader_compiles_and_draws_its_own_way() {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let registry = ShaderRegistry::new(Vec::new(), &BuiltinShader::ALL.into_iter().collect::<BTreeSet<_>>());
    renderer.set_custom_shaders(dispatch_code(&registry));
    // Compiling takes seconds, on its own thread, drawing with the shaders as they were meanwhile.
    let world = World::new();
    for _ in 0..1200 {
        let _ = renderer.render(&world, 0.0);
        match renderer.custom_shaders() {
            CustomShaderState::Compiling => std::thread::sleep(std::time::Duration::from_millis(50)),
            CustomShaderState::Failed(error) => panic!("the built-in shaders did not compile: {error}"),
            _ => break,
        }
    }
    assert_eq!(*renderer.custom_shaders(), CustomShaderState::Ready);

    let standard = ball(&mut renderer, &registry, None, 1.0);
    keep_image("builtin standard.png", &standard);
    for shader in BuiltinShader::ALL {
        let drawn = ball(&mut renderer, &registry, Some(shader), 1.0);
        keep_image(&format!("builtin {}.png", shader.name()), &drawn);
        let [r, g, b] = patch(&drawn, LIT, 3);
        let title = shader.title();
        match shader {
            BuiltinShader::Water => {
                let teal = drawn.chunks(4).filter(|p| p[2] > p[0].saturating_add(15) && p[1] > p[0]).count();
                assert!(teal > 2000, "{title}: much of the ball is blue-green water: {teal} pixels");
            }
            BuiltinShader::Hologram => assert!(b > r + 60 && g > r + 40, "{title}: a cyan glow: {:?}", [r, g, b]),
            BuiltinShader::Lava => {
                let hottest = drawn.chunks(4).map(|p| i32::from(p[0]) - i32::from(p[2])).max().unwrap();
                assert!(hottest > 120, "{title}: somewhere it glows orange: {hottest}");
            }
            BuiltinShader::Sway => {
                // The ball's top sways, so it is somewhere else a moment later.
                let later = ball(&mut renderer, &registry, Some(shader), 1.4);
                let moved = drawn.iter().zip(&later).filter(|(a, b)| a.abs_diff(**b) > 30).count();
                assert!(moved > 200, "{title}: the ball moves: {moved} pixels changed");
            }
            BuiltinShader::Toon | BuiltinShader::Storybook | BuiltinShader::Triplanar => {
                let changed = drawn.iter().zip(&standard).filter(|(a, b)| a.abs_diff(**b) > 20).count();
                assert!(changed > 1000, "{title}: drawn other than the standard shader: {changed} pixels differ");
            }
        }
    }

    // On glass, the hologram's light adds to what shows through it, and water tints and ripples it.
    let opaque = ball(&mut renderer, &registry, Some(BuiltinShader::Hologram), 1.0);
    let see_through = ball_of(&mut renderer, &registry, Some(BuiltinShader::Hologram), 1.0, true);
    keep_image("builtin hologram glass.png", &see_through);
    let (solid, through) = (patch(&opaque, (320, 180), 4), patch(&see_through, (320, 180), 4));
    assert_ne!(solid, through, "the glass hologram shows what is behind it");
    assert!(through[2] > through[0] + 30, "and glows cyan over it: {through:?}");
    let water = ball_of(&mut renderer, &registry, Some(BuiltinShader::Water), 1.0, true);
    keep_image("builtin water glass.png", &water);

    // Water and lava as they are meant to be seen: a wide sheet with many vertices, low against the sun.
    let water = scene(&mut renderer, &registry, BuiltinShader::Water, false);
    keep_image("showcase water.png", &water);
    let clear = scene(&mut renderer, &registry, BuiltinShader::Water, true);
    keep_image("showcase water glass.png", &clear);
    let lava = scene(&mut renderer, &registry, BuiltinShader::Lava, false);
    keep_image("showcase lava.png", &lava);
    let glowing = lava.chunks(4).filter(|p| p[0] > 200 && p[0] > p[2].saturating_add(80)).count();
    assert!(glowing > 500, "Lava: its cracks glow orange: {glowing} pixels");
}

/// A flat sheet 60 metres wide and `cells` cells across, facing up.
fn sheet(cells: u32) -> MeshData {
    let mut mesh = MeshData::default();
    let side = 60.0;
    for z in 0..=cells {
        for x in 0..=cells {
            let (u, v) = (x as f32 / cells as f32, z as f32 / cells as f32);
            mesh.vertices.push(Vertex {
                position: [(u - 0.5) * side, 0.0, (v - 0.5) * side],
                normal: [0.0, 1.0, 0.0],
                uv: [u * side, v * side],
                tangent: [1.0, 0.0, 0.0, 1.0],
                ..Vertex::default()
            });
        }
    }
    let row = cells + 1;
    for z in 0..cells {
        for x in 0..cells {
            let at = z * row + x;
            mesh.indices.extend([at, at + row, at + 1, at + 1, at + row, at + row + 1]);
        }
    }
    mesh
}

/// `shader` on a wide sheet seen from a little above, the sun low ahead;
/// with `glass`, over sand that rises out of it to a beach in front.
fn scene(renderer: &mut Renderer, registry: &ShaderRegistry, shader: BuiltinShader, glass: bool) -> Vec<u8> {
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.22, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 2.5, -12.0), look, Vec3::ONE)));
    let sheet = renderer.add_mesh(&sheet(240)).unwrap();
    let (number, info) = registry.get(ShaderRef::Builtin(shader)).unwrap();
    let mut desc = MaterialDesc { base_color: [1.0; 4], glass, ior: 1.33, shader: number, ..MaterialDesc::default() };
    desc.max_offset = info.max_offset;
    for (slot, property) in desc.properties.iter_mut().zip(&info.properties) {
        *slot = default_on_gpu(property);
    }
    let material = renderer.add_material(&desc).unwrap();
    world.spawn((MeshRenderer::new(sheet, material), Transform::default()));
    if glass {
        let plane = renderer.primitive(Primitive::Plane).unwrap();
        let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
        let sand = renderer.add_material(&sand).unwrap();
        // Rising towards the camera: three metres down far off, above the water at the camera's feet.
        let slope = Quat::from_rotation_x(0.12);
        let at = Transform::new(Vec3::new(0.0, -1.6, 4.0), slope, Vec3::new(60.0, 1.0, 60.0));
        world.spawn((MeshRenderer::new(plane, sand), at));
    }
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.25, -0.3, -1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    world.insert_resource(post);
    // Long enough for the bounced light to settle.
    for _ in 0..60 {
        renderer.render(&world, 3.0).unwrap();
    }
    renderer.read_pixels().unwrap().2
}
