//! Shader packs' passes drawn on a real GPU, offscreen: a pass at each of
//! the five points of the frame, a pack's own targets, kept from frame to
//! frame or not, the normals and last frame's colour a pass reads, what a
//! program that does not compile says, and the built-in Evening pack over
//! a world of blocks. Each test skips itself (with a note) on machines
//! without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MeshRenderer, PostProcess, Primitive, Renderer,
    RendererSettings, Sky,
    shader_packs::{self, CompiledPack, PackFiles, PackManifest, WornShaderPack},
};
use voxear_scene::Transform;

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn keep_image(name: &str, pixels: &[u8], (width, height): (u32, u32)) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn rgb_at(pixels: &[u8], x: u32, y: u32) -> [i32; 3] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from)
}

/// The middle of the picture, where the cube is, and a corner, where the sky is.
const CUBE: (u32, u32) = (WIDTH / 2, HEIGHT / 2);
const SKY: (u32, u32) = (12, 12);

/// A pack of `programs` and its `manifest`, in a folder of its own, compiled.
fn pack(manifest: &str, programs: &[(&str, &str)]) -> anyhow::Result<CompiledPack> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("pack.toml"), manifest).unwrap();
    for (name, text) in programs {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    let parsed = PackManifest::parse(manifest)?;
    shader_packs::compile(&parsed, &PackFiles::Folder(dir.path().to_path_buf()))
}

/// One pass at `at` of `program`, reading `reads`.
fn one_pass(at: &str, reads: &str, program: &str) -> CompiledPack {
    let manifest = format!("[[pass]]\nname = \"Test\"\nprogram = \"test.slang\"\nat = \"{at}\"\nreads = [{reads}]\n");
    pack(&manifest, &[("test.slang", program)]).unwrap_or_else(|error| panic!("{error:#}"))
}

/// A white cube in front of the camera under a sun and a gradient sky,
/// with fixed exposure, no TAA unless `taa`, and nothing that reads the
/// screen.
fn cube_world(renderer: &mut Renderer, taa: bool, fog: bool) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::ZERO)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = renderer.add_material(&MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], ..MaterialDesc::default() });
    world.spawn((
        MeshRenderer::new(cube, white.unwrap()),
        Transform::new(Vec3::new(0.0, 0.0, 12.0), Quat::IDENTITY, Vec3::splat(5.0)),
    ));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let mode = if taa { AntiAliasingMode::Taa } else { AntiAliasingMode::Off };
    let anti_aliasing = AntiAliasing { mode, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    post.fog.enabled = fog;
    post.fog.volumetric = false;
    post.fog.density = 0.08;
    post.fog.height_falloff = 0.0001;
    world.insert_resource(post);
    world
}

fn render(renderer: &mut Renderer, world: &World, frames: u32) -> Vec<u8> {
    let mut time = 0.0;
    for _ in 0..frames {
        time += 1.0 / 60.0;
        renderer.render(world, time).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

fn wear(world: &mut World, pack: CompiledPack) -> u64 {
    let generation = pack.generation;
    world.insert_resource(WornShaderPack::new(Arc::new(pack)));
    generation
}

/// Paints what is drawn red and leaves the sky as it is.
const RED: &str = "float4 run(PassInput input)\n{\n    return isSky(input.uv) ? sceneColor(input.uv) : float4(1.5, 0.0, 0.0, 1.0);\n}\n";

#[test]
fn a_pass_after_lighting_is_fogged_and_one_after_fog_is_not() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, true);
    let plain = render(&mut renderer, &world, 3);
    let drawn = wear(&mut world, one_pass("after_lighting", "\"color\", \"depth\"", RED));
    let lit = render(&mut renderer, &world, 3);
    assert_eq!(renderer.shader_pack_drawn(), Some(drawn), "its pipelines made");
    wear(&mut world, one_pass("after_fog", "\"color\", \"depth\"", RED));
    let fogged = render(&mut renderer, &world, 3);
    keep_image("pack after lighting.png", &lit, (WIDTH, HEIGHT));
    keep_image("pack after fog.png", &fogged, (WIDTH, HEIGHT));
    let ([r, g, b], [fr, fg, fb]) = (rgb_at(&lit, CUBE.0, CUBE.1), rgb_at(&fogged, CUBE.0, CUBE.1));
    eprintln!("after lighting {r} {g} {b}; after fog {fr} {fg} {fb}");
    assert!(fr > fg + 120 && fr > fb + 120, "red with nothing over it after the fog: {fr} {fg} {fb}");
    assert!(r > g + 25 && r > b + 25, "the cube red under the fog: {r} {g} {b}");
    assert!(g > fg + 20 && b > fb + 20, "with the fog's colour over it: {r} {g} {b}");
    // The sky is left as it was, at both points.
    let sky = rgb_at(&plain, SKY.0, SKY.1);
    for picture in [&lit, &fogged] {
        let seen = rgb_at(picture, SKY.0, SKY.1);
        assert!((0..3).all(|c| (seen[c] - sky[c]).abs() <= 2), "the sky: {seen:?} against {sky:?}");
    }
}

#[test]
fn a_pass_after_taa_works_at_the_size_the_picture_is_shown() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, true, false);
    // The scene drawn at half the size, for TAA to upscale.
    if let Some(post) = world.resource_mut::<PostProcess>() {
        post.anti_aliasing.render_scale = 0.5;
    }
    // A checkerboard a pixel a square, which a pass before the upscale could not draw.
    let checks = "float4 run(PassInput input)\n{\n    int2 at = int2(input.pixel);\n    \
                  return ((at.x + at.y) & 1) != 0 ? float4(4.0, 4.0, 4.0, 1.0) : float4(0.0, 0.0, 0.0, 1.0);\n}\n";
    wear(&mut world, one_pass("after_taa", "", checks));
    let pixels = render(&mut renderer, &world, 3);
    keep_image("pack after taa.png", &pixels, (WIDTH, HEIGHT));
    let mut alternating = 0;
    for x in 40..80 {
        let (a, b) = (rgb_at(&pixels, x, 60)[0], rgb_at(&pixels, x + 1, 60)[0]);
        alternating += u32::from((a - b).abs() > 120);
    }
    assert!(alternating >= 38, "pixel by pixel at the shown size: {alternating} of 40");
}

#[test]
fn a_final_pass_works_on_the_finished_picture_as_shown() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let plain = render(&mut renderer, &world, 3);
    let invert = "float4 run(PassInput input)\n{\n    return float4(1.0 - sceneColor(input.uv).rgb, 1.0);\n}\n";
    wear(&mut world, one_pass("final", "\"color\"", invert));
    let inverted = render(&mut renderer, &world, 3);
    keep_image("pack final.png", &inverted, (WIDTH, HEIGHT));
    // In linear colours: one minus the other, as sRGB codes.
    let linear = |code: i32| {
        let c = code as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    for (x, y) in [CUBE, SKY, (WIDTH - 20, HEIGHT - 20)] {
        let (before, after) = (rgb_at(&plain, x, y), rgb_at(&inverted, x, y));
        for c in 0..3 {
            let sum = linear(before[c]) + linear(after[c]);
            assert!((sum - 1.0).abs() < 0.03, "at {x}, {y}: {before:?} and {after:?} add up to {sum}");
        }
    }
}

#[test]
fn a_pass_at_the_tonemap_draws_the_picture_in_the_composites_place() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let plain = render(&mut renderer, &world, 3);
    // The engine's own curve, as the composite has it with nothing else on, draws the same picture.
    let same = "float4 run(PassInput input)\n{\n    float3 light = sceneColor(input.uv).rgb * frame()->exposure;\n    \
                return float4(tonemapSoft(light), 1.0);\n}\n";
    wear(&mut world, one_pass("tonemap", "\"color\"", same));
    let copied = render(&mut renderer, &world, 3);
    for (x, y) in [CUBE, SKY, (WIDTH - 20, HEIGHT - 20)] {
        let (before, after) = (rgb_at(&plain, x, y), rgb_at(&copied, x, y));
        for c in 0..3 {
            assert!((before[c] - after[c]).abs() <= 2, "at {x}, {y}: {before:?} became {after:?}");
        }
    }
    // A curve of its own, with the glow round what is bright read apart.
    let mut post = world.resource::<PostProcess>().unwrap().clone();
    post.bloom.enabled = true;
    world.insert_resource(post);
    let own = "float4 run(PassInput input)\n{\n    float3 light = sceneColor(input.uv).rgb * frame()->exposure;\n    \
               return float4(tonemapFilmic(light).bgr + sceneBloom(input.uv) * 0.0, 1.0);\n}\n";
    wear(&mut world, one_pass("tonemap", "\"color\", \"bloom\"", own));
    let swapped = render(&mut renderer, &world, 3);
    keep_image("pack tonemap.png", &swapped, (WIDTH, HEIGHT));
    let (before, after) = (rgb_at(&plain, SKY.0, SKY.1), rgb_at(&swapped, SKY.0, SKY.1));
    assert!(before[2] > before[0] + 20, "the sky is blue: {before:?}");
    assert!(after[0] > after[2] + 20, "the pack's curve swaps blue for red: {after:?}");
    // The glow read on its own and shown as the engine would is the picture blurred: as bright where it is smooth.
    let glow = "float4 run(PassInput input)\n{\n    \
                return float4(tonemapSoft(sceneBloom(input.uv) * frame()->exposure), 1.0);\n}\n";
    wear(&mut world, one_pass("tonemap", "\"bloom\"", glow));
    let glowing = render(&mut renderer, &world, 3);
    keep_image("pack tonemap bloom.png", &glowing, (WIDTH, HEIGHT));
    for (x, y) in [(WIDTH / 2, 20), (WIDTH - 20, HEIGHT - 20)] {
        let (before, after) = (rgb_at(&plain, x, y), rgb_at(&glowing, x, y));
        for c in 0..3 {
            assert!((before[c] - after[c]).abs() <= 25, "at {x}, {y}: the glow {after:?} against {before:?}");
        }
    }
    // A pack that reads the glow before the tonemap is told where it can.
    let manifest = "[[pass]]\nname = \"Early\"\nprogram = \"test.slang\"\nat = \"after_taa\"\nreads = [\"bloom\"]\n";
    let error = PackManifest::parse(manifest).expect_err("bloom before the tonemap");
    assert!(format!("{error:#}").contains("tonemap"), "{error:#}");
}

#[test]
fn a_sky_program_draws_the_sky_behind_everything_and_is_given_the_engines_by_its_parts() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let plain = render(&mut renderer, &world, 3);
    // Red above the horizon, the engine's sky below it.
    let red = "float3 run(SkySurface surface, SkyLighting light)\n{\n    \
               return surface.direction.y > 0.0 ? float3(0.5 / frame()->exposure, 0.0, 0.0) : light.engine;\n}\n";
    wear(&mut world, surfaces("sky = \"sky.slang\"", &[("sky.slang", red)]));
    let reddened = render(&mut renderer, &world, 3);
    keep_image("pack sky.png", &reddened, (WIDTH, HEIGHT));
    let above = rgb_at(&reddened, SKY.0, SKY.1);
    assert!(above[0] > above[2] + 60 && above[0] > above[1] + 60, "above the horizon the sky is red: {above:?}");
    for (x, y) in [CUBE, (12, HEIGHT - 12)] {
        let (before, after) = (rgb_at(&plain, x, y), rgb_at(&reddened, x, y));
        for c in 0..3 {
            assert!((before[c] - after[c]).abs() <= 2, "at {x}, {y}: {before:?} became {after:?}");
        }
    }
    // Its parts laid as the engine lays them come to the engine's sky: the difference, made large, adds nothing to a
    // picture of where each pixel is.
    let program = |difference: &str| {
        format!(
            "float3 run(SkySurface surface, SkyLighting light)\n{{\n    \
             float3 laid = skyLayered(light.sky + light.disc + light.stars, light.clouds, light.weather, \
             surface.underClouds);\n    \
             return {difference} + float3(0.0, surface.uv.y * 0.5 / frame()->exposure, 0.0);\n}}\n"
        )
    };
    let with = program("abs(laid - light.engine) * 1000.0");
    wear(&mut world, surfaces("sky = \"sky.slang\"", &[("sky.slang", &with)]));
    let difference = render(&mut renderer, &world, 3);
    let without = program("float3(0.0)");
    wear(&mut world, surfaces("sky = \"sky.slang\"", &[("sky.slang", &without)]));
    let places = render(&mut renderer, &world, 3);
    for (x, y) in [SKY, (SKY.0, HEIGHT - 12), (WIDTH - 12, HEIGHT / 3)] {
        let (exact, laid) = (rgb_at(&places, x, y), rgb_at(&difference, x, y));
        assert!(exact.iter().zip(laid).all(|(a, b)| (a - b).abs() <= 1), "at {x}, {y}: {laid:?} against {exact:?}");
    }
    let (top, bottom) = (rgb_at(&places, SKY.0, SKY.1), rgb_at(&places, SKY.0, HEIGHT - 12));
    assert!(bottom[1] > top[1] + 60, "uv goes from 0 at the top to 1 at the bottom: {top:?}, {bottom:?}");
}

#[test]
fn a_target_written_at_half_size_is_read_by_a_later_pass() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let manifest = r#"
[[pass]]
name = "Mask"
program = "mask.slang"
at = "after_lighting"
reads = ["depth"]
writes = { target = "mask", format = "r16f", scale = 0.5 }

[[pass]]
name = "Show"
program = "show.slang"
at = "final"
reads = ["mask"]
"#;
    let mask = "float4 run(PassInput input)\n{\n    return float4(isSky(input.uv) ? 1.0 : 0.0, 0.0, 0.0, 1.0);\n}\n";
    let show = "float4 run(PassInput input)\n{\n    return float4(mask.sample(input.uv).rrr, 1.0);\n}\n";
    let compiled = pack(manifest, &[("mask.slang", mask), ("show.slang", show)]).unwrap();
    wear(&mut world, compiled);
    let pixels = render(&mut renderer, &world, 3);
    keep_image("pack target.png", &pixels, (WIDTH, HEIGHT));
    assert_eq!(rgb_at(&pixels, CUBE.0, CUBE.1), [0, 0, 0], "the cube is no sky");
    assert!(rgb_at(&pixels, SKY.0, SKY.1).iter().all(|&c| c > 250), "the sky");
}

#[test]
fn a_kept_target_carries_over_from_frame_to_frame() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let manifest = r#"
[[pass]]
name = "Count"
program = "count.slang"
at = "final"
reads = ["count"]
writes = { target = "count", format = "r16f", kept = true }

[[pass]]
name = "Show"
program = "show.slang"
at = "final"
reads = ["count"]
"#;
    let count = "float4 run(PassInput input)\n{\n    return float4(min(count.last(input.uv).r + 0.1, 1.0), 0.0, 0.0, 1.0);\n}\n";
    let show = "float4 run(PassInput input)\n{\n    return float4(count.sample(input.uv).rrr, 1.0);\n}\n";
    wear(&mut world, pack(manifest, &[("count.slang", count), ("show.slang", show)]).unwrap());
    let first = rgb_at(&render(&mut renderer, &world, 1), CUBE.0, CUBE.1)[0];
    let fourth = rgb_at(&render(&mut renderer, &world, 3), CUBE.0, CUBE.1)[0];
    eprintln!("after one frame {first}, after four {fourth}");
    // 0.1 and 0.4, as sRGB codes.
    assert!((first - 89).abs() <= 3, "a tenth after the first frame: {first}");
    assert!((fourth - 170).abs() <= 3, "four tenths after the fourth: {fourth}");
}

#[test]
fn a_pass_that_reads_normals_has_the_depth_passes_write_them() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let normals = "float4 run(PassInput input)\n{\n    return float4(sceneNormal(input.uv) * 0.5 + 0.5, 1.0);\n}\n";
    wear(&mut world, one_pass("final", "\"normals\"", normals));
    let pixels = render(&mut renderer, &world, 3);
    keep_image("pack normals.png", &pixels, (WIDTH, HEIGHT));
    // The cube's face turned to the camera faces back along -z.
    let [r, g, b] = rgb_at(&pixels, CUBE.0, CUBE.1);
    assert!((r - 188).abs() < 6 && (g - 188).abs() < 6 && b < 6, "facing the camera: {r} {g} {b}");
}

#[test]
fn a_pass_reads_last_frames_colour_once_there_is_one() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let last = "float4 run(PassInput input)\n{\n    return luminance(lastColor(input.uv).rgb) > 0.0 ? 1.0 : float4(0.0, 0.0, 0.0, 1.0);\n}\n";
    wear(&mut world, one_pass("final", "\"history\"", last));
    let first = rgb_at(&render(&mut renderer, &world, 1), CUBE.0, CUBE.1);
    let second = rgb_at(&render(&mut renderer, &world, 1), CUBE.0, CUBE.1);
    assert_eq!(first, [0, 0, 0], "no last frame for the first");
    assert_eq!(second, [255, 255, 255], "the first frame's for the second");
}

#[test]
fn a_pack_taken_off_leaves_the_frame_as_it_was() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let plain = render(&mut renderer, &world, 3);
    wear(&mut world, one_pass("after_fog", "\"color\", \"depth\"", RED));
    let red = render(&mut renderer, &world, 3);
    assert_ne!(plain, red);
    world.remove_resource::<WornShaderPack>();
    let again = render(&mut renderer, &world, 3);
    assert_eq!(renderer.shader_pack_drawn(), None, "its pipelines let go");
    // As it was but for the dither, which moves from frame to frame.
    let most = plain.iter().zip(&again).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
    assert!(most <= 2, "the frame as it was: {most} apart at most");
}

#[test]
fn a_pack_compiled_again_is_drawn_as_it_now_is_and_one_that_fails_says_where() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = cube_world(&mut renderer, false, false);
    let dir = tempfile::tempdir().unwrap();
    let manifest = "[[pass]]\nname = \"Tint\"\nprogram = \"tint.slang\"\nat = \"final\"\nreads = [\"color\"]\n";
    std::fs::write(dir.path().join("pack.toml"), manifest).unwrap();
    let parsed = PackManifest::parse(manifest).unwrap();
    let files = PackFiles::Folder(dir.path().to_path_buf());
    let tint = |color: &str| format!("float4 run(PassInput input)\n{{\n    return float4({color}, 1.0);\n}}\n");
    std::fs::write(dir.path().join("tint.slang"), tint("1.0, 0.0, 0.0")).unwrap();
    let started = std::time::Instant::now();
    let red = shader_packs::compile(&parsed, &files).unwrap();
    eprintln!("compiled in {:.2} s", started.elapsed().as_secs_f32());
    let red_generation = wear(&mut world, red);
    assert_eq!(rgb_at(&render(&mut renderer, &world, 2), CUBE.0, CUBE.1), [255, 0, 0]);
    // Saved again: compiled again and drawn from the next frame.
    std::fs::write(dir.path().join("tint.slang"), tint("0.0, 0.0, 1.0")).unwrap();
    let blue = wear(&mut world, shader_packs::compile(&parsed, &files).unwrap());
    assert_ne!(blue, red_generation);
    assert_eq!(rgb_at(&render(&mut renderer, &world, 2), CUBE.0, CUBE.1), [0, 0, 255]);
    assert_eq!(renderer.shader_pack_drawn(), Some(blue));
    // The same again compiles nothing.
    let started = std::time::Instant::now();
    shader_packs::compile(&parsed, &files).unwrap();
    assert!(started.elapsed().as_secs_f32() < 0.2, "from what was compiled before");
    // A mistake: Slang's message names the file and its line, and the pack worn stays as it was.
    std::fs::write(dir.path().join("tint.slang"), "float4 run(PassInput input)\n{\n    return nothing;\n}\n").unwrap();
    let error = format!("{:#}", shader_packs::compile(&parsed, &files).unwrap_err());
    assert!(error.contains("the pass `Tint`") && error.contains("tint.slang:3"), "{error}");
    assert_eq!(rgb_at(&render(&mut renderer, &world, 1), CUBE.0, CUBE.1), [0, 0, 255]);
    // A program that reads what its pass does not list is told so.
    std::fs::write(dir.path().join("tint.slang"), tint("sceneNormal(input.uv)")).unwrap();
    let error = format!("{:#}", shader_packs::compile(&parsed, &files).unwrap_err());
    assert!(error.contains("lists `normals` in its `reads`"), "{error}");
}

/// A field of blocks with a row of trees across it, their crowns full of
/// gaps, seen from the grass towards a low sun behind them.
fn trees_at_evening(renderer: &mut Renderer) -> World {
    use voxear_render::{BlockLook, SurfaceLook};
    use voxear_terrain::blocks::{
        BlockWorld,
        file::{BlockFile, BlockKind},
    };
    let mut file = BlockFile::default();
    file.ground.size = [64, 64];
    file.ground.height = 4;
    let first = file.blocks.len() as u16 + 1;
    file.blocks.push(BlockKind::colored("Log", [0.36, 0.26, 0.17]));
    file.blocks.push(BlockKind::colored("Leaves", [0.22, 0.48, 0.18]));
    let (log, leaves) = (first, first + 1);
    let mut blocks = file.make_ground(None);
    for trunk in (14..52).step_by(7) {
        for y in 4..11 {
            blocks.set([trunk, y, 40], log);
        }
        for y in 9..15i32 {
            for z in 37..44i32 {
                for x in trunk as i32 - 3..trunk as i32 + 4 {
                    // Leaves with gaps, a pattern of holes the sun comes through.
                    let hash =
                        (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ z.wrapping_mul(83_492_791)) as u32;
                    if hash % 5 > 1 {
                        blocks.set([x, y, z], leaves);
                    }
                }
            }
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            let material =
                renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
            SurfaceLook {
                material: material.unwrap(),
                size: 1.0,
                sharpness: 0.0,
                triplanar: true,
                tracks: 0.0,
                fills_in: 0.0,
                liquid: Default::default(),
                glitter: false,
                // The leaves' look is drawn as leaves are.
                leafy: look.color == [0.22, 0.48, 0.18],
                mixed: false,
                tint: 0,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook {
        material: default,
        surfaces: Arc::from(surfaces),
        voxels: Default::default(),
        motions: Arc::from([]),
        light: Default::default(),
    };
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    // From the grass, looking along +z a little up at the crowns, the sun low behind them.
    let up = Quat::from_rotation_x(-0.22);
    world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 6.0, 18.0), up, Vec3::ONE)));
    let towards_sun = Vec3::new(0.0, 0.32, 1.0).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::new(1.0, 0.9, 0.75), 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Taa, sharpness: 0.0, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

/// Each pixel's brightness, and how much redder than blue it is, averaged over a part of the picture.
fn averages(pixels: &[u8], xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) -> (f32, f32) {
    let (mut light, mut warm, mut count) = (0.0, 0.0, 0.0);
    for y in ys {
        for x in xs.clone() {
            let [r, g, b] = rgb_at(pixels, x, y).map(|c| c as f32);
            light += (r + g + b) / 3.0;
            warm += r - b;
            count += 1.0;
        }
    }
    (light / count, warm / count)
}

#[test]
fn the_evening_pack_warms_a_world_of_blocks_and_lays_rays_through_its_leaves() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = trees_at_evening(&mut renderer);
    let plain = render(&mut renderer, &world, 24);
    let manifest = PackManifest::builtin("evening").unwrap();
    // From its folder in the checkout, as it is now, where there is one.
    let files = match shader_packs::builtin_folder("evening") {
        Some(folder) => PackFiles::Folder(folder),
        None => PackFiles::Embedded(shader_packs::builtin_files("evening").unwrap()),
    };
    let evening = Arc::new(shader_packs::compile(&manifest, &files).unwrap());
    assert_eq!(evening.modules.len(), 3, "its programs for block faces, water and the sky, beside its passes");
    world.insert_resource(WornShaderPack::new(evening.clone()));
    let worn = render(&mut renderer, &world, 24);
    // The same, without the rays.
    let mut no_rays = WornShaderPack::new(evening);
    no_rays.options[0] = 0.0;
    world.insert_resource(no_rays);
    let without_rays = render(&mut renderer, &world, 24);
    // And without the leaves' glow.
    let mut no_glow = WornShaderPack::new(world.resource::<WornShaderPack>().unwrap().pack.clone());
    no_glow.options[0] = 0.0;
    no_glow.options[4] = 0.0;
    world.insert_resource(no_glow);
    let without_glow = render(&mut renderer, &world, 24);
    keep_image("evening without glow.png", &without_glow, (WIDTH, HEIGHT));
    let crowns = |pixels: &[u8]| averages(pixels, WIDTH / 2 - 60..WIDTH / 2 + 60, 60..85).0;
    let (glowing, dull) = (crowns(&without_rays), crowns(&without_glow));
    eprintln!("the crowns against the sun: {glowing:.1} with their glow, {dull:.1} without");
    assert!(glowing > dull + 5.0, "leaves glow against the sun: {glowing} against {dull}");
    keep_image("evening plain.png", &plain, (WIDTH, HEIGHT));
    keep_image("evening worn.png", &worn, (WIDTH, HEIGHT));
    keep_image("evening without rays.png", &without_rays, (WIDTH, HEIGHT));
    let all = (0..WIDTH, 0..HEIGHT);
    let ((plain_light, plain_warm), (worn_light, worn_warm)) =
        (averages(&plain, all.0.clone(), all.1.clone()), averages(&worn, all.0, all.1));
    eprintln!("plain: light {plain_light:.1}, warm {plain_warm:.1}; worn: light {worn_light:.1}, warm {worn_warm:.1}");
    assert!(worn_warm > plain_warm + 8.0, "warmer: {worn_warm} against {plain_warm}");
    // The rays fall from the sun, behind the crowns in the middle of the picture, and brighten it there most.
    let (sun_x, edge) = (WIDTH / 2 - 30..WIDTH / 2 + 30, 0..30);
    let rays = |pixels: &[u8], xs: std::ops::Range<u32>| averages(pixels, xs, 40..HEIGHT - 40).0;
    let (middle, side) = (
        rays(&worn, sun_x.clone()) - rays(&without_rays, sun_x),
        rays(&worn, edge.clone()) - rays(&without_rays, edge),
    );
    eprintln!("the rays brighten the middle by {middle:.1}, the side by {side:.1}");
    assert!(middle > 4.0, "rays towards the sun: {middle}");
    assert!(middle > side * 2.0, "most towards the sun: {middle} against {side}");
}

/// The trees at evening as a plain picture: no TAA, fixed exposure.
fn trees_plain(renderer: &mut Renderer) -> World {
    let mut world = trees_at_evening(renderer);
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    world.insert_resource(post);
    world
}

/// The grass in front of the trees, a crown's leaves, and the sky over them.
const GRASS: (u32, u32) = (WIDTH / 2, HEIGHT - 12);
const CROWN: (u32, u32) = (40, 72);
const OVER: (u32, u32) = (WIDTH / 2, 8);

/// Whether two pictures are the same but for the dither and a few pixels at
/// shadows' edges, which the sun's shadow pages draw a little differently
/// from run to run: how many differ by more than a little, and by how much
/// on average.
fn alike(a: &[u8], b: &[u8]) -> (usize, f32) {
    let pixels: Vec<u8> =
        a.chunks(4).zip(b.chunks(4)).map(|(a, b)| (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap()).collect();
    let off = pixels.iter().filter(|&&d| d > 4).count();
    (off, pixels.iter().map(|&d| f32::from(d)).sum::<f32>() / pixels.len() as f32)
}

/// A pack of `[surface]` programs alone, in a folder of its own.
fn surfaces(surface: &str, programs: &[(&str, &str)]) -> CompiledPack {
    let manifest = format!(
        "[options]\nlevel = {{ kind = \"number\", range = [0.0, 2.0], default = 1.0 }}\n[surface]\n{surface}\n"
    );
    pack(&manifest, programs).unwrap_or_else(|error| panic!("{error:#}"))
}

#[test]
fn a_blocks_program_lights_every_face_and_a_leaves_program_the_leaves() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = trees_plain(&mut renderer);
    let plain = render(&mut renderer, &world, 3);
    let red =
        "float3 run(BlockSurface surface, BlockLighting light)\n{\n    return float3(options().level, 0.0, 0.0);\n}\n";
    let blue = "float3 run(BlockSurface surface, BlockLighting light)\n{\n    return float3(0.0, 0.0, 1.0);\n}\n";
    let compiled = surfaces(
        "blocks = \"faces.slang\"\nleaves = \"leaves.slang\"",
        &[("faces.slang", red), ("leaves.slang", blue)],
    );
    assert_eq!(compiled.modules.len(), 1, "the engine's block shader, compiled with them");
    wear(&mut world, compiled);
    let worn = render(&mut renderer, &world, 3);
    assert!(renderer.pack_surfaces_drawn());
    keep_image("surfaces worn.png", &worn, (WIDTH, HEIGHT));
    let ([r, g, b], [lr, lg, lb]) = (rgb_at(&worn, GRASS.0, GRASS.1), rgb_at(&worn, CROWN.0, CROWN.1));
    assert!(r > g + 120 && g == b, "the grass lit red by its program: {r} {g} {b}");
    assert!(lb > lr + 120 && lr == lg, "the leaves blue by theirs: {lr} {lg} {lb}");
    let (sky, before) = (rgb_at(&worn, OVER.0, OVER.1), rgb_at(&plain, OVER.0, OVER.1));
    assert!((0..3).all(|c| (sky[c] - before[c]).abs() <= 2), "the sky as it was: {sky:?} against {before:?}");
    // An option the program reads, set as whoever plays sets it.
    if let Some(worn) = world.resource_mut::<WornShaderPack>() {
        worn.options[0] = 0.25;
    }
    let dim = rgb_at(&render(&mut renderer, &world, 2), GRASS.0, GRASS.1)[0];
    assert!(dim < r - 40, "a quarter as bright: {dim} against {r}");
    // Taken off, block faces are the engine's again.
    world.remove_resource::<WornShaderPack>();
    let again = render(&mut renderer, &world, 3);
    assert!(!renderer.pack_surfaces_drawn());
    let (off, mean) = alike(&plain, &again);
    assert!(off < (WIDTH * HEIGHT / 200) as usize && mean < 0.6, "as it was: {off} pixels off, {mean:.2} on average");
}

#[test]
fn a_blocks_program_is_given_the_engines_light_whole_and_by_its_terms() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = trees_plain(&mut renderer);
    let plain = render(&mut renderer, &world, 10);
    let whole = "float3 run(BlockSurface surface, BlockLighting light)\n{\n    return light.engine;\n}\n";
    wear(&mut world, surfaces("blocks = \"faces.slang\"", &[("faces.slang", whole)]));
    let engine = render(&mut renderer, &world, 10);
    let terms = "float3 run(BlockSurface surface, BlockLighting light)\n{\n    \
                 return light.sunTerm + light.ambientTerm + light.reflectionTerm + light.lamps + light.emission;\n}\n";
    wear(&mut world, surfaces("blocks = \"faces.slang\"", &[("faces.slang", terms)]));
    let summed = render(&mut renderer, &world, 10);
    // Lit by the sun alone, as the program works it out from what the face is and the light it sees.
    let own = "float3 run(BlockSurface surface, BlockLighting light)\n{\n    \
               return surface.color * light.sunLight * light.shadow * saturate(dot(surface.normal, light.sunDirection)) / 3.14159;\n}\n";
    wear(&mut world, surfaces("blocks = \"faces.slang\"", &[("faces.slang", own)]));
    let sunlit = render(&mut renderer, &world, 3);
    keep_image("surfaces sun alone.png", &sunlit, (WIDTH, HEIGHT));
    keep_image("surfaces plain.png", &plain, (WIDTH, HEIGHT));
    keep_image("surfaces engine.png", &engine, (WIDTH, HEIGHT));
    keep_image("surfaces terms.png", &summed, (WIDTH, HEIGHT));
    for (name, picture) in [("the engine's", &engine), ("its terms added up", &summed)] {
        let (off, mean) = alike(&plain, picture);
        eprintln!("{name}: {off} pixels off, {mean:.3} on average");
        assert!(off < (WIDTH * HEIGHT / 200) as usize && mean < 0.6, "{name}: {off} pixels off, {mean:.2} on average");
    }
    // The sun's light alone: the sunlit grass between the trunks lit, though less than with the sky's light too, and
    // the grass in the crowns' shadow black.
    let (lit, plain_lit) = (rgb_at(&sunlit, WIDTH / 2, 140), rgb_at(&plain, WIDTH / 2, 140));
    let shade = rgb_at(&sunlit, GRASS.0, GRASS.1);
    eprintln!("sunlit grass {lit:?} against {plain_lit:?}; shade {shade:?}");
    assert!(lit[1] > 100 && lit[1] < plain_lit[1], "sunlit: {lit:?} against {plain_lit:?}");
    assert!(shade.iter().all(|&c| c < 8), "in shadow: {shade:?}");
}

/// A world of blocks with a pond in it, standing in the sea, seen from over its edge: the pond near, the sea round it.
fn pond_in_the_sea(renderer: &mut Renderer) -> World {
    use voxear_render::{BlockLook, Ocean, SurfaceLook};
    use voxear_terrain::blocks::{
        BlockWorld,
        file::{BlockFile, BlockKind},
    };
    let mut file = BlockFile::default();
    file.ground.size = [24, 24];
    file.ground.height = 4;
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.03, 0.3, 0.42]) });
    let water = file.blocks.len() as u16;
    let mut blocks = file.make_ground(None);
    for z in 8..16 {
        for x in 8..16 {
            blocks.set([x, 3, z], water);
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            let material =
                renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
            SurfaceLook {
                material: material.unwrap(),
                size: 1.0,
                sharpness: 0.0,
                triplanar: true,
                tracks: 0.0,
                fills_in: 0.0,
                liquid: Default::default(),
                glitter: false,
                leafy: false,
                mixed: false,
                tint: 0,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook {
        material: default,
        surfaces: Arc::from(surfaces),
        voxels: Default::default(),
        motions: Arc::from([]),
        light: Default::default(),
    };
    let mut world = World::new();
    let corner = Transform::from_position(Vec3::new(-12.0, 0.0, -12.0));
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, corner));
    world.spawn((Ocean::default(), Transform::default()));
    let towards = (Vec3::new(0.0, 3.0, 4.0) - Vec3::new(0.0, 14.0, -18.0)).normalize();
    let rotation = Quat::from_rotation_y(towards.x.atan2(towards.z)) * Quat::from_rotation_x(-towards.y.asin());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 14.0, -18.0), rotation, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.7, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    world.insert_resource(post);
    world
}

/// How many pixels are mostly red, and how many red and blue alike, over green.
fn red_and_magenta(pixels: &[u8]) -> (usize, usize) {
    let (mut red, mut magenta) = (0, 0);
    for pixel in pixels.chunks(4) {
        let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i32::from);
        red += usize::from(r > g + 80 && b < g + 20);
        magenta += usize::from(r > g + 80 && b > g + 80);
    }
    (red, magenta)
}

#[test]
fn a_water_program_lights_the_sea_and_a_worlds_water_and_is_given_the_engines_light() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = pond_in_the_sea(&mut renderer);
    let plain = render(&mut renderer, &world, 6);
    keep_image("water plain.png", &plain, (WIDTH, HEIGHT));
    let kinds = "float3 run(WaterSurface surface, WaterLighting light)\n{\n    float level = options().level;\n    \
                 if ((surface.kind & WATER_IS_BLOCKS) != 0)\n    {\n        return float3(level, 0.0, 0.0);\n    }\n    \
                 return (surface.kind & WATER_IS_SEA) != 0 ? float3(level, 0.0, level) : light.engine;\n}\n";
    let compiled = surfaces("water = \"water.slang\"", &[("water.slang", kinds)]);
    assert_eq!(compiled.modules.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["water"]);
    wear(&mut world, compiled);
    let painted = render(&mut renderer, &world, 6);
    keep_image("water painted.png", &painted, (WIDTH, HEIGHT));
    assert!(renderer.pack_surfaces_drawn());
    let (pond, sea) = red_and_magenta(&painted);
    eprintln!("{pond} pixels of the pond red, {sea} of the sea magenta");
    assert!(pond > 1500 && sea > 4000, "{pond} of the pond, {sea} of the sea");
    assert_eq!(red_and_magenta(&plain), (0, 0));
    // Given the engine's colour back, as it was; and its colour without foam laid back over the foam, near enough.
    let whole = "float3 run(WaterSurface surface, WaterLighting light)\n{\n    return light.engine;\n}\n";
    wear(&mut world, surfaces("water = \"water.slang\"", &[("water.slang", whole)]));
    let engine = render(&mut renderer, &world, 6);
    keep_image("water engine.png", &engine, (WIDTH, HEIGHT));
    let (off, mean) = alike(&plain, &engine);
    eprintln!("the engine's: {off} pixels off, {mean:.3} on average");
    assert!(
        off < (WIDTH * HEIGHT / 100) as usize && mean < 1.0,
        "the engine's: {off} pixels off, {mean:.2} on average"
    );
    world.remove_resource::<WornShaderPack>();
    render(&mut renderer, &world, 2);
    assert!(!renderer.pack_surfaces_drawn());
}
