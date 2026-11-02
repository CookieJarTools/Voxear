//! End-to-end checks on a real GPU, rendering offscreen. Each test skips
//! itself (with a note) on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    Camera, CookedModel, Light, MaterialDesc, MeshRenderer, Primitive, RenderStats, Renderer, RendererSettings,
    ViewOptions,
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

#[test]
fn an_obj_new_to_its_place_in_the_list_does_not_fade_in() {
    let Some(mut renderer) = renderer() else { return };
    // A small sphere far off, drawn at a coarse LOD, until it settles.
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -4.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let far =
        world.spawn((MeshRenderer { mesh: sphere, material }, Transform::from_position(Vec3::new(0.0, 0.0, 80.0))));
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 4);

    // Another, close up, takes its place in the list, as when a scene opens
    // in place of another: it shows at once, not fading in from the old LOD.
    world.despawn(far);
    world
        .spawn((MeshRenderer { mesh: sphere, material }, Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0))));
    let swapped = run(&mut renderer, &world, &mut time, 1);
    assert_eq!(swapped.fade_draws, 0, "{swapped:?}");
}

/// A camera-facing quad, two-sided so winding never hides it, with a base
/// colour texture and a flat normal map, embedded as PNGs.
fn textured_quad() -> Vec<u8> {
    let mut buffer = Vec::new();
    let positions = [[-1f32, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
    for p in positions {
        p.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    }
    for _ in 0..4 {
        [0f32, 0.0, -1.0].iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    }
    for uv in [[0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]] {
        uv.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    }
    for index in [0u16, 1, 2, 0, 2, 3, 0, 2, 1, 0, 3, 2] {
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    let png = |pixel: [u8; 4]| {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 4, 4);
            encoder.set_color(png::ColorType::Rgba);
            let pixels: Vec<u8> = (0..16).flat_map(|_| pixel).collect();
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        bytes
    };
    let data = |bytes: &[u8], mime: &str| format!("data:{mime};base64,{}", base64::encode(bytes));
    format!(
        r#"{{
          "asset": {{ "version": "2.0" }},
          "buffers": [{{ "uri": "{buffer}", "byteLength": {length} }}],
          "bufferViews": [
            {{ "buffer": 0, "byteOffset": 0, "byteLength": 48 }},
            {{ "buffer": 0, "byteOffset": 48, "byteLength": 48 }},
            {{ "buffer": 0, "byteOffset": 96, "byteLength": 32 }},
            {{ "buffer": 0, "byteOffset": 128, "byteLength": 24 }}
          ],
          "accessors": [
            {{ "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-1, -1, 0], "max": [1, 1, 0] }},
            {{ "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3" }},
            {{ "bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2" }},
            {{ "bufferView": 3, "componentType": 5123, "count": 12, "type": "SCALAR" }}
          ],
          "images": [{{ "uri": "{colour}" }}, {{ "uri": "{normal}" }}],
          "textures": [{{ "source": 0 }}, {{ "source": 1 }}],
          "materials": [{{
            "pbrMetallicRoughness": {{ "baseColorTexture": {{ "index": 0 }}, "metallicFactor": 0, "roughnessFactor": 1 }},
            "normalTexture": {{ "index": 1 }}
          }}],
          "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }}, "indices": 3, "material": 0 }}] }}],
          "nodes": [{{ "mesh": 0 }}],
          "scenes": [{{ "nodes": [0] }}]
        }}"#,
        buffer = data(&buffer, "application/octet-stream"),
        length = buffer.len(),
        colour = data(&png([220, 120, 40, 255]), "image/png"),
        normal = data(&png([128, 128, 255, 255]), "image/png"),
    )
    .into_bytes()
}

/// Renders the quad with its textures as given, or cooked (BC7 and BC5 with
/// mips, written out and read back), and returns the pixel at the centre and
/// at a corner.
fn render_quad(cooked: bool) -> Option<([u8; 4], [u8; 4])> {
    let mut renderer = renderer()?;
    let gltf = textured_quad();
    let mut model = voxear_render::prepare_model("Quad.gltf", &mut |_| Ok(gltf.clone())).unwrap();
    let imported = if cooked {
        model.compress_textures();
        let bytes = model.cook().unwrap().to_bytes();
        renderer.upload_cooked(&CookedModel::from_bytes("Quad.gltf", &bytes).unwrap()).unwrap()
    } else {
        renderer.upload_model(&model).unwrap()
    };
    Some(centre_and_corner(&mut renderer, &imported))
}

fn centre_and_corner(renderer: &mut Renderer, imported: &voxear_render::ImportedModel) -> ([u8; 4], [u8; 4]) {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    world.spawn((
        MeshRenderer { mesh: imported.nodes[0].mesh, material: imported.nodes[0].material },
        Transform::IDENTITY,
    ));
    run(renderer, &world, &mut 0.0, 8);
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    (at(width / 2, height / 2), at(2, 2))
}

/// A cooked quad whose 512×512 gradient texture spans many GDeflate tiles.
fn large_cooked_quad() -> CookedModel {
    let gltf = textured_quad();
    let mut model = voxear_render::prepare_model("Quad.gltf", &mut |_| Ok(gltf.clone())).unwrap();
    model.replace_texture(
        0,
        512,
        512,
        (0..512 * 512).flat_map(|i| [(i % 512 / 2) as u8, (i / 1024) as u8, 90, 255]).collect(),
    );
    model.compress_textures();
    CookedModel::from_bytes("Quad.gltf", &model.cook().unwrap().to_bytes()).unwrap()
}

#[test]
fn the_gpu_and_the_cpu_decompress_to_the_same_image() {
    let Some(mut renderer) = renderer() else { return };
    if !renderer.decompresses_on_gpu() {
        eprintln!("skipping: this GPU cannot decompress GDeflate");
        return;
    }
    let model = large_cooked_quad();
    assert!(model.is_compressed());
    let on_gpu = renderer.upload_cooked(&model).unwrap();
    assert!(renderer.asset_usage().decompressed_on_gpu > 512 * 512, "the GPU did the decompressing");
    let gpu = centre_and_corner(&mut renderer, &on_gpu);

    let Some(mut cpu_renderer) = renderer_without_gpu_decompression() else { return };
    let on_cpu = cpu_renderer.upload_cooked(&model).unwrap();
    assert_eq!(cpu_renderer.asset_usage().decompressed_on_gpu, 0);
    assert_eq!(centre_and_corner(&mut cpu_renderer, &on_cpu), gpu);
    assert_ne!(gpu.0, gpu.1, "the quad covers the centre");
}

fn renderer_without_gpu_decompression() -> Option<Renderer> {
    let mut renderer = renderer()?;
    renderer.set_gpu_decompression(false);
    Some(renderer)
}

#[test]
fn cooked_textures_look_like_their_source() {
    let (Some(source), Some(cooked)) = (render_quad(false), render_quad(true)) else { return };
    assert_ne!(source.0, source.1, "the quad covers the centre");
    let difference = (0..3).map(|c| source.0[c].abs_diff(cooked.0[c])).max().unwrap();
    assert!(difference <= 4, "BC7 and BC5 stay within a few levels: {:?} vs {:?}", source.0, cooked.0);
}

#[test]
fn the_ui_draws_over_the_frame() {
    use voxear_ui::{Color, Style, TextStyle, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let style = Style::column().at(20.0, 20.0).width(200.0).padding(16.0).radius(12.0);
    let panel = ui.panel(root, style.background(Color::hex(0xff0000)).border(2.0, Color::hex(0x00ff00)));
    ui.label(panel, "Voxear", TextStyle::default().size(24.0));
    let (width, height) = (640, 360);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);

    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let [x, y, w, h] = ui.rect(panel).map(|v| v as u32);
    assert_eq!(at(x + w - 20, y + h / 2), [255, 0, 0], "the panel is filled");
    assert_eq!(at(x, y + h / 2), [0, 255, 0], "its left edge is the border");
    assert_eq!(at(x + w - 1, y + h / 2), [0, 255, 0], "and its right");
    let corner = at(x, y);
    assert!(corner[0] < 200 && corner[1] < 200, "the corner is rounded off, got {corner:?}");
    let text = (x + 16..x + 100).flat_map(|tx| (y + 16..y + h - 16).map(move |ty| (tx, ty)));
    let brightest = text.map(|(tx, ty)| at(tx, ty).into_iter().min().unwrap()).max().unwrap();
    assert!(brightest > 230, "white text is drawn, brightest {brightest}");

    renderer.set_ui(&voxear_ui::UiFrame::default()).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = (((y + h / 2) * width + x + w - 20) * 4) as usize;
    assert_ne!(pixels[i..i + 3], [255, 0, 0], "an empty frame clears the UI");
}

/// The game's UI is laid out for its own area and drawn moved there, under
/// the window's UI, each with its own glyph atlas.
#[test]
fn the_games_ui_draws_in_its_area_under_the_windows() {
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let (width, height) = (640, 360);
    let mut game = Ui::new();
    let root = game.root();
    game.panel(root, Style::default().at(10.0, 10.0).width(60.0).height(60.0).background(Color::hex(0xff0000)));
    renderer.set_game_ui(&game.frame(300, 200, 1.0), [200, 100]).unwrap();
    let mut window = Ui::new();
    let root = window.root();
    window.panel(root, Style::default().at(240.0, 100.0).width(20.0).height(200.0).background(Color::hex(0x0000ff)));
    renderer.set_ui(&window.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);

    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(225, 135), [255, 0, 0], "the game's panel, moved to its area at 200, 100");
    assert_ne!(at(35, 35), [255, 0, 0], "not where it would be in the window's own space");
    assert_eq!(at(250, 135), [0, 0, 255], "the window's UI over it");

    renderer.set_game_ui(&voxear_ui::UiFrame::default(), [0, 0]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = ((135 * width + 225) * 4) as usize;
    assert_ne!(pixels[i..i + 3], [255, 0, 0], "an empty frame takes the game's UI away");
}

/// A second output, as the window of a panel floated on the desktop, draws
/// the views put there and a UI of its own, and is cleared for its UI when
/// no view draws into it.
#[test]
fn a_second_output_draws_its_own_views_and_ui() {
    use voxear_render::OutputId;
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let camera = world.spawn((Camera::default(), Transform::IDENTITY));
    let magenta = [255, 0, 255];
    renderer.set_clear_color([1.0, 0.0, 1.0, 1.0]);
    renderer.set_view_rect(Some([0, 0, 100, 100]));
    let (width, height) = (320, 200);
    let second = renderer.add_offscreen_output(width, height).unwrap();
    assert_eq!(renderer.outputs(), [OutputId::MAIN, second]);
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(20.0, 20.0).width(60.0).height(60.0).background(Color::hex(0xff0000)));
    renderer.set_output_ui(second, &ui.frame(width, height, 1.0)).unwrap();
    let options = ViewOptions { camera: Some(camera), ..ViewOptions::default() };
    renderer.set_more_views_in(&[(second, [100, 0, 200, 200], options)]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);

    let (w, h, pixels) = renderer.read_output_pixels(second).unwrap();
    assert_eq!((w, h), (width, height));
    let at = |pixels: &[u8], width: u32, x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(&pixels, width, 50, 50), [255, 0, 0], "its own UI");
    assert_eq!(at(&pixels, width, 10, 150), magenta, "cleared around its view");
    assert_ne!(at(&pixels, width, 200, 100), magenta, "its view is drawn");
    let (main_width, _, main) = renderer.read_pixels().unwrap();
    assert_ne!(at(&main, main_width, 50, 50), [255, 0, 0], "the first output has none of its UI");
    assert_eq!(at(&main, main_width, 200, 100), magenta, "nor its view");

    renderer.set_more_views_in(&[]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_output_pixels(second).unwrap();
    assert_eq!(at(&pixels, width, 200, 100), magenta, "with no view it is cleared for its UI");
    assert_eq!(at(&pixels, width, 50, 50), [255, 0, 0]);

    renderer.remove_output(second);
    run(&mut renderer, &world, &mut 0.0, 1);
    assert!(renderer.read_output_pixels(second).is_err(), "gone");
    assert_eq!(renderer.outputs(), [OutputId::MAIN]);
}

/// Captures read an area of the output back, before the UI draws or after,
/// in the order asked for.
#[test]
fn captures_read_an_area_back_before_or_after_the_ui() {
    use voxear_render::Capture;
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(100.0).height(100.0).background(Color::hex(0xff0000)));
    renderer.set_ui(&ui.frame(640, 360, 1.0)).unwrap();
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 1);
    assert!(renderer.take_capture().is_none(), "none asked for");

    renderer.request_capture(Capture { area: Some([10, 10, 40, 30]), before_ui: true });
    renderer.request_capture(Capture { area: Some([10, 10, 40, 30]), before_ui: false });
    renderer.request_capture(Capture { area: Some([600, 340, 100, 100]), before_ui: false });
    run(&mut renderer, &world, &mut time, 3);
    let under = renderer.take_capture().expect("the view before the UI");
    let over = renderer.take_capture().expect("the window after it");
    let edge = renderer.take_capture().expect("an area off the edge");
    assert!(renderer.take_capture().is_none());

    assert_eq!((under.width, under.height, under.rgba.len()), (40, 30, 40 * 30 * 4));
    let first = |image: &voxear_render::CapturedImage| [image.rgba[0], image.rgba[1], image.rgba[2], image.rgba[3]];
    assert_ne!(first(&under)[..3], [255, 0, 0], "under the panel, the scene");
    assert_eq!(first(&over), [255, 0, 0, 255], "over it, the panel, opaque");
    assert_eq!((edge.width, edge.height), (40, 20), "kept inside the output");
}

/// The frame debugger lists the frame's passes and their images, and shows
/// the one looked at as it stood after its pass, in the channels asked for.
#[test]
fn the_frame_debugger_shows_an_image_after_its_pass() {
    use voxear_render::{Channels, FrameLook};
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(100.0).height(100.0).background(Color::hex(0xff0000)));
    let shown = ui.image(root);
    ui.set_inline_style(shown, "position: absolute; left: 320px; top: 0px; width: 320px; height: 180px");
    renderer.set_ui(&ui.frame(640, 360, 1.0)).unwrap();
    renderer.set_frame_debugger(true, None);
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 1);

    let passes = renderer.frame_passes();
    let names: Vec<&str> = passes.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Opaque (early)") && names.contains(&"Composite"), "{names:?}");
    let opaque = passes.iter().find(|p| p.name == "Opaque (early)").unwrap();
    let depth = opaque.images.iter().find(|i| i.name == "depth").expect("the opaque pass draws depth");
    assert!(depth.writes && depth.depth && depth.channels == 1 && depth.viewable);
    assert_eq!(depth.size, [640, 360]);
    let ui_pass = passes.iter().position(|p| p.name == "UI").unwrap();
    let output = passes[ui_pass].images.iter().position(|i| i.name == "output").unwrap();
    assert_eq!(passes[ui_pass].images[output].used, "Drawn into");
    assert!(renderer.frame_debugger_view().is_none(), "nothing looked at yet");

    let look = FrameLook { pass: ui_pass, image: output, ..FrameLook::default() };
    renderer.set_frame_debugger(true, Some(look));
    run(&mut renderer, &world, &mut time, 1);
    let (image, size) = renderer.frame_debugger_view().expect("a view of what is looked at");
    assert_eq!(size, [640, 360]);
    ui.set_image(shown, Some(image));
    renderer.set_ui(&ui.frame(640, 360, 1.0)).unwrap();
    run(&mut renderer, &world, &mut time, 1);
    let at = |pixels: &[u8], x: u32, y: u32| {
        let i = ((y * 640 + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    assert_eq!(at(&pixels, 50, 50), [255, 0, 0], "the red panel itself");
    let red = at(&pixels, 345, 25);
    assert!(red[0] > 240 && red[1] < 16 && red[2] < 16, "the view of the window after the UI, at half size: {red:?}");

    renderer.set_frame_debugger(true, Some(FrameLook { channels: Channels::Green, ..look }));
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    assert!(at(&pixels, 345, 25).iter().all(|&c| c < 16), "red has no green, so the green channel is black");

    // Depth, looked at with more exposure, shows the wall in grey.
    let pass = renderer.frame_passes().iter().position(|p| p.name == "Opaque (early)").unwrap();
    let image = renderer.frame_passes()[pass].images.iter().position(|i| i.name == "depth").unwrap();
    let depth = FrameLook { pass, image, mip: 0, channels: Channels::Red, exposure: 8.0 };
    renderer.set_frame_debugger(true, Some(depth));
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let wall = at(&pixels, 400, 100);
    assert!(wall[0] > 0 && wall[0] == wall[1] && wall[1] == wall[2], "the wall's depth as grey: {wall:?}");
    assert_eq!(at(&pixels, 600, 20), [0, 0, 0], "the sky is as far as depth goes");

    renderer.set_frame_debugger(false, None);
    run(&mut renderer, &world, &mut time, 1);
    assert!(renderer.frame_passes().is_empty() && renderer.frame_debugger_view().is_none(), "off, it keeps nothing");
}

/// A panel with `backdrop-filter` shows the view under it blurred: the
/// wall's sharp edge behind it comes out soft, while beside it the edge stays
/// sharp. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the image.
#[test]
fn backdrops_blur_the_view_behind_them() {
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let (width, height) = (640, 360);
    run(&mut renderer, &world, &mut 0.0, 2);
    let (_, _, plain) = renderer.read_pixels().unwrap();
    // The wall's right edge, as the largest jump between neighbours along a row.
    let edge = |pixels: &[u8], y: u32, from: u32, to: u32| {
        let at = |x: u32| {
            let i = ((y * width + x) * 4) as usize;
            pixels[i] as i32 + pixels[i + 1] as i32 + pixels[i + 2] as i32
        };
        (from..to).map(|x| (at(x + 1) - at(x)).abs()).max().unwrap()
    };
    let (row, left, right) = (150, 250, 390);
    let sharp = edge(&plain, row, left, right);
    assert!(sharp > 60, "the wall has a sharp edge to blur: {sharp}");

    let mut ui = Ui::new();
    let root = ui.root();
    let mut style = Style::column().at(left as f32, 100.0).width((right - left) as f32).height(100.0).radius(10.0);
    style.background = Some(Color::hex(0x000000).with_alpha(0));
    style.backdrop_blur = 16.0;
    ui.panel(root, style);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 2);
    let (_, _, frosted) = renderer.read_pixels().unwrap();
    let soft = edge(&frosted, row, left + 12, right - 12);
    assert!(soft * 3 < sharp, "behind the panel the edge is blurred: {soft} against {sharp}");
    assert!(edge(&frosted, 260, left, right) > 60, "below the panel it is still sharp");
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("backdrop.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&frosted).unwrap();
    }
}

/// A styled list of 100,000 rows and a tree, scrolled, with a selection,
/// drawn over a frame. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the image.
#[test]
fn lists_and_trees_draw_clipped_and_selected() {
    use voxear_ui::{ItemFns, NodeId, Style, TextStyle, TreeItems, Ui};

    struct Folders;
    impl TreeItems for Folders {
        fn children(&self, parent: Option<u64>) -> Vec<u64> {
            match parent {
                None => (1..=6).collect(),
                Some(folder @ 1..=6) => (1..=3).map(|i| folder * 10 + i).collect(),
                Some(_) => Vec::new(),
            }
        }
        fn make(&mut self, ui: &mut Ui, row: NodeId) {
            ui.label(row, "", TextStyle::default());
        }
        fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
            let label = ui.children(row)[0];
            let name = if item < 10 { format!("Folder {item}") } else { format!("Obj {item}") };
            ui.set_text(label, &name);
        }
    }

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let sheet = r#"
        .side { padding: 6px; background-color: #111216; border-radius: 10px; font-size: 12px; color: #c8c9d0; }
        .list-row { padding: 0px 8px; border-radius: 6px; }
        .list-row:hover { background-color: #1c1d22; }
        .list-row:selected { background-color: #2b2f55; color: #ffffff; }
        .tree-arrow { color: #6b6c75; }
    "#;
    ui.add_sheet(root, voxear_ui::StyleSheet::parse("Test.vss", sheet));
    let side = ui.panel(root, Style::default().at(16.0, 16.0).width(260.0).height(300.0));
    ui.add_class(side, "side");
    let list = ui.list_view(
        side,
        22.0,
        100_000,
        ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                ui.label(row, "", TextStyle::default());
            },
            bind: |ui: &mut Ui, row: NodeId, index: usize| {
                let label = ui.children(row)[0];
                ui.set_text(label, &format!("Item {index}"));
            },
        },
    );
    ui.set_style(list, Style::column().scroll().grow());
    let other = ui.panel(root, Style::default().at(300.0, 16.0).width(260.0).height(300.0));
    ui.add_class(other, "side");
    let tree = ui.tree_view(other, 22.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.set_expanded(tree, 2, true);

    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    ui.set_scroll(list, [0.0, 22.0 * 49_998.0]);
    ui.set_selection(list, &[50_000]);
    ui.frame(width, height, 1.0);
    let rows: Vec<NodeId> =
        ui.children(tree).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    let file = *rows.iter().find(|&&r| ui.rect(r)[1] > ui.rect(rows[0])[1] + 50.0).unwrap();
    let [x, y, w, h] = ui.rect(file);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("lists.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }

    let at = |x: f32, y: f32| {
        let i = ((y as u32 * width + x as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let selected = [0x2b, 0x2f, 0x55];
    let [lx, ly, lw, _] = ui.viewport(list);
    assert_eq!(at(lx + lw - 30.0, ly + 2.0 * 22.0 + 11.0), selected, "item 50,000 is third from the top, selected");
    assert_eq!(at(x + w - 20.0, y + h / 2.0), selected, "the clicked tree row is selected");
    assert_eq!(ui.selected_items(tree).len(), 1);
}

/// The input controls with the default theme: a focused text field with a
/// selection, a toggle, a slider, a foldout and a button. Set
/// `VOXEAR_TEST_OUTPUT` to keep the image.
#[test]
fn controls_draw_with_the_default_theme() {
    use voxear_ui::{Key, Modifiers, Shape, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <Panel style="position: absolute; left: 16px; top: 16px; width: 280px; padding: 12px; gap: 8px;
                          background-color: #0f1011; border-radius: 12px; border: 1px solid #1f2023">
                <TextField name="name" placeholder="Name" value="Firefly spawner" />
                <Toggle name="fly" value="true">Can fly</Toggle>
                <Toggle name="static" class="checkbox" value="true">Static</Toggle>
                <Slider name="speed" min="0" max="20" value="6" step="0.5" />
                <Segmented value="1">
                    <Segment icon="select" />
                    <Segment icon="move" />
                    <Segment icon="rotate" />
                    <Segment icon="scale" />
                    <Segment text="Pivot" icon="pivot" />
                </Segmented>
                <Foldout text="Movement" open="false">
                    <Label>Speed</Label>
                </Foldout>
                <Button>Bake path</Button>
                <Button style="border: 1px dashed #2e3035; background-color: transparent; color: #8a8f98">
                    <Icon icon="plus" />
                    <Label>Add Component</Label>
                </Button>
            </Panel>
            <Panel name="overlay" style="position: absolute; left: 340px; top: 40px; width: 200px; height: 120px;
                          border-radius: 12px; background-color: rgba(15, 16, 17, 0.82);
                          border: 1px solid #2e3035; box-shadow: 0px 16px 40px rgba(0, 0, 0, 0.4)" />
            <Panel name="canvas" style="position: absolute; left: 340px; top: 200px; width: 120px; height: 120px" />
        </vui>"#;
    let document = ui.load_markup(root, "Form.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let canvas = ui.find(document, "canvas").unwrap();
    let red = voxear_ui::Color::hex(0xeb5757);
    ui.set_shapes(
        canvas,
        vec![
            Shape::Line { from: [60.0, 60.0], to: [100.0, 60.0], width: 2.0, color: red },
            Shape::Circle { center: [100.0, 60.0], radius: 9.0, fill: red, border: None },
            Shape::Circle {
                center: [60.0, 60.0],
                radius: 40.0,
                fill: voxear_ui::Color::TRANSPARENT,
                border: Some((1.0, voxear_ui::Color::hex(0x2e3035))),
            },
        ],
    );
    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    let name = ui.find(document, "name").unwrap();
    ui.focus(name);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Right);
    ui.set_modifiers(Modifiers { shift: true, control: true });
    ui.key_down(Key::Right);
    ui.set_modifiers(Modifiers::default());
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("controls.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let at = |x: f32, y: f32| {
        let i = ((y as u32 * width + x as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let accent = [0x79, 0x88, 0xff];
    let fly = ui.find(document, "fly").unwrap();
    let pill = ui.children(fly)[0];
    let [x, y, w, h] = ui.rect(pill);
    assert_eq!((w, h), (26.0, 14.0), "a toggle is a 26 by 14 pill");
    assert_eq!(at(x + 5.0, y + 7.0), accent, "an on switch fills with the accent");
    assert_eq!(at(x + 19.0, y + 7.0), [0xff, 0xff, 0xff], "with its knob over on the right");
    let check_box = ui.children(ui.find(document, "static").unwrap())[0];
    let [x, y, ..] = ui.rect(check_box);
    assert_eq!(at(x + 2.0, y + 7.0), accent, "a ticked checkbox fills with the accent");
    let [x, y, _, h] = ui.rect(name);
    assert_eq!(at(x, y + h / 2.0), accent, "the focused field's border is the accent");
    let [x, y, w, h] = ui.rect(ui.find(document, "overlay").unwrap());
    let (below, beside) = (at(x + w / 2.0, y + h + 10.0), at(x + w + 90.0, y + h + 10.0));
    assert!(below[0] + 8 < beside[0], "the overlay's shadow darkens what is under it: {below:?} {beside:?}");
    let [x, y, ..] = ui.rect(canvas);
    assert_eq!(at(x + 100.0, y + 60.0), [0xeb, 0x57, 0x57], "a circle drawn on a canvas");
    assert_eq!(at(x + 80.0, y + 60.0), [0xeb, 0x57, 0x57], "and the line to it");
    assert!(ui.take_warnings().is_empty());
}

/// A split view, tabs, a grid-laid inspector and an open dropdown, as the
/// editor will use them. Set `VOXEAR_TEST_OUTPUT` to keep the image.
#[test]
fn panes_draw_an_editor_like_layout() {
    use voxear_ui::Ui;

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <SplitView name="split" size="170" style="position: absolute; left: 12px; top: 12px; right: 12px; bottom: 12px;
                                                       font-size: 12px; color: #c8c9d0">
                <Panel style="padding: 10px; gap: 6px; background-color: #111216; border-radius: 10px; flex-grow: 1">
                    <Label style="font-weight: 600; color: #e8e8ec">Hierarchy</Label>
                    <Label>Main Camera</Label>
                    <Label>Sun</Label>
                    <Label>Lamp</Label>
                </Panel>
                <Tabs selected="2" style="padding: 10px; background-color: #111216; border-radius: 10px; flex-grow: 1">
                    <Tab text="Scene" />
                    <Tab text="Game" />
                    <Tab text="Inspector">
                        <Panel style="display: grid; grid-template-columns: 90px 1fr; gap: 8px; align-items: center">
                            <Label>Name</Label><TextField value="Lamp" />
                            <Label>Shading</Label>
                            <Dropdown name="shading" value="0"><Option>Lit</Option><Option>Unlit</Option><Option>Wireframe</Option></Dropdown>
                            <Label>Intensity</Label><Slider min="0" max="10" value="3" />
                            <Label>Shadows</Label><Toggle value="true">Cast</Toggle>
                        </Panel>
                    </Tab>
                </Tabs>
            </SplitView>
        </vui>"#;
    let document = ui.load_markup(root, "Editor.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    let shading = ui.find(document, "shading").unwrap();
    ui.open_dropdown(shading);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("panes.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let split = ui.find(document, "split").unwrap();
    let divider = ui.children(split)[1];
    let [x, y, _, h] = ui.rect(divider);
    let i = (((y + h / 2.0) as u32 * width + x as u32 + 1) * 4) as usize;
    assert_eq!(pixels[i..i + 3], [0x25, 0x27, 0x2b], "the divider between the panes");
    assert!(ui.take_warnings().is_empty());
}

/// A docked editor layout, caught mid-drag: the ghost of a tab and the
/// preview of where it would land. Set `VOXEAR_TEST_OUTPUT` to keep the image.
#[test]
fn docking_draws_areas_and_a_drag_preview() {
    use voxear_ui::{NodeId, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <DockSpace name="dock" layout="row(150, [hierarchy, project], column(230, [scene, game], row(250, [console], [inspector])))"
                       style="position: absolute; left: 8px; top: 8px; right: 8px; bottom: 8px; font-size: 12px; color: #c8c9d0">
                <DockPanel id="hierarchy" title="Hierarchy"><Label>Main Camera</Label><Label>Sun</Label><Label>Lamp</Label></DockPanel>
                <DockPanel id="project" title="Project"><Label>Assets</Label></DockPanel>
                <DockPanel id="scene" title="Scene" />
                <DockPanel id="game" title="Game" />
                <DockPanel id="console" title="Console"><Label>Loaded game scripts</Label></DockPanel>
                <DockPanel id="inspector" title="Inspector">
                    <Panel style="display: grid; grid-template-columns: 70px 1fr; gap: 6px; align-items: center">
                        <Label>Name</Label><TextField value="Lamp" />
                        <Label>Intensity</Label><Slider min="0" max="10" value="3" />
                    </Panel>
                </DockPanel>
            </DockSpace>
        </vui>"#;
    let document = ui.load_markup(root, "Editor.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    let dock = ui.find(document, "dock").unwrap();
    fn find_tab(ui: &Ui, node: NodeId, title: &str) -> Option<NodeId> {
        if ui.has_class(node, "tab") && ui.text(node) == title {
            return Some(node);
        }
        ui.children(node).into_iter().find_map(|child| find_tab(ui, child, title))
    }
    let console = find_tab(&ui, dock, "Console").unwrap();
    let scene_area = ui.parent(ui.parent(find_tab(&ui, dock, "Scene").unwrap()).unwrap()).unwrap();
    let [cx, cy, cw, ch] = ui.rect(console);
    let [sx, sy, _, sh] = ui.rect(scene_area);
    ui.pointer_move(cx + cw / 2.0, cy + ch / 2.0);
    ui.pointer_button(true);
    ui.pointer_move(sx + 40.0, sy + sh / 2.0 - 10.0);
    ui.pointer_move(sx + 30.0, sy + sh / 2.0);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("dock.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let [x, y, ..] = ui.rect(scene_area);
    let i = (((y + 4.0) as u32 * width + x as u32) * 4) as usize;
    assert!(pixels[i + 2] > 0xa0, "the preview's accent edge over the scene's left half");
    assert!(ui.take_warnings().is_empty());
}

/// The 3D view drawn into part of the output, as the editor's Scene panel
/// does, and through a chosen camera rather than the first one.
#[test]
fn the_view_draws_into_a_rect_through_a_chosen_camera() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -100.0))));
    let chosen = world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material =
        renderer.add_material(&MaterialDesc { base_color: [1.0, 0.1, 0.1, 1.0], ..Default::default() }).unwrap();
    world.spawn((MeshRenderer { mesh: cube, material }, Transform::IDENTITY));
    renderer.set_view_rect(Some([320, 40, 200, 200]));
    renderer.set_view_options(ViewOptions { camera: Some(chosen), ..ViewOptions::default() });
    run(&mut renderer, &world, &mut 0.0, 3);
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(10, 10), [0, 0, 0], "outside the view is cleared");
    assert_eq!(at(600, 300), [0, 0, 0]);
    let centre = at(420, 140);
    // Lit only by the sky, so a dim red, but clearly red.
    assert!(
        centre[0] > 60 && centre[0] > 2 * centre[1],
        "the near camera sees the red cube filling the view's middle: {centre:?}"
    );
    let sky = at(325, 45);
    assert!(sky[2] > sky[0], "the view's corner shows sky: {sky:?}");
}

#[test]
fn a_preview_draws_an_object_on_its_background_for_the_ui_to_show() {
    use voxear_render::{PreviewOptions, PreviewPart};
    use voxear_ui::{ImageRef, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let red =
        renderer.add_material(&MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    let part = PreviewPart { mesh: cube, material: red, transform: voxear_core::math::Mat4::IDENTITY };
    let options = PreviewOptions { size: 128, background: [0.0, 0.0, 1.0, 1.0], ..PreviewOptions::default() };
    let id = renderer.add_preview(vec![part], options);
    assert!(renderer.preview(id).is_none(), "drawn with the next frame");
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 1);
    let image = renderer.preview(id).expect("drawn");
    assert_eq!(image.size, 128);

    // Shown at its own size by an image in the UI.
    let mut ui = Ui::new();
    let root = ui.root();
    let shown = ui.image(root);
    ui.set_inline_style(shown, "position: absolute; left: 20px; top: 20px; width: 128px; height: 128px");
    ui.set_image(shown, Some(ImageRef { texture: image.texture.0, uv: image.uv }));
    let _ = Style::default();
    let (width, height) = (640, 360);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("preview.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let middle = at(20 + 64, 20 + 64);
    assert!(middle[0] > 80 && middle[0] > middle[1] * 2 && middle[0] > middle[2] * 2, "the red cube, got {middle:?}");
    let corner = at(22, 22);
    assert!(corner[2] > 200 && corner[0] < 40, "the blue background, got {corner:?}");

    // Asked again with a clear background, the cube's edges are smooth and the rest shows what is behind.
    renderer.set_preview(id, None, PreviewOptions { background: [0.0; 4], ..options });
    run(&mut renderer, &world, &mut time, 1);
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = ((22 * width + 22) * 4) as usize;
    assert!(pixels[i + 2] < 200, "no blue once the background is clear, got {:?}", &pixels[i..i + 4]);
    renderer.free_preview(id);
    assert!(renderer.preview(id).is_none());
}

#[test]
fn a_previews_pixels_read_back_and_save_as_a_png() {
    use voxear_render::{PreviewOptions, PreviewPart};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let green =
        renderer.add_material(&MaterialDesc { base_color: [0.1, 0.8, 0.2, 1.0], ..MaterialDesc::default() }).unwrap();
    let part = PreviewPart { mesh: sphere, material: green, transform: voxear_core::math::Mat4::IDENTITY };
    let id = renderer.add_preview(vec![part], PreviewOptions { size: 64, ..PreviewOptions::default() });
    renderer.read_preview(id);
    let mut time = 0.0;
    let mut pixels = None;
    for _ in 0..6 {
        run(&mut renderer, &world, &mut time, 0);
        pixels = pixels.or_else(|| renderer.preview_pixels(id));
    }
    let pixels = pixels.expect("read back within a few frames");
    assert_eq!((pixels.size, pixels.rgba.len()), (64, 64 * 64 * 4));
    let at = |x: usize, y: usize| &pixels.rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    let middle = at(32, 32);
    assert!(middle[3] == 255 && middle[1] > middle[0] && middle[1] > middle[2], "a green ball, got {middle:?}");
    assert_eq!(at(1, 1)[3], 0, "on a clear background");
    assert!(renderer.preview_pixels(id).is_none(), "handed over once");

    let file = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("preview-ball.png");
    pixels.save_png(&file).unwrap();
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&file).unwrap()));
    let info = decoder.read_info().unwrap().info().clone();
    assert_eq!((info.width, info.height, info.color_type), (64, 64, png::ColorType::Rgba));
}

#[test]
fn a_full_preview_atlas_grows_and_draws_everything_again() {
    use voxear_render::{PreviewOptions, PreviewPart};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let part = PreviewPart { mesh: cube, material, transform: voxear_core::math::Mat4::IDENTITY };
    // The first atlas holds sixteen of these; the twentieth makes it grow.
    let ids: Vec<_> = (0..20)
        .map(|_| renderer.add_preview(vec![part], PreviewOptions { size: 256, ..PreviewOptions::default() }))
        .collect();
    let mut time = 0.0;
    for _ in 0..8 {
        run(&mut renderer, &world, &mut time, 0);
        for &id in &ids {
            renderer.preview(id);
        }
    }
    let images: Vec<_> = ids.iter().map(|&id| renderer.preview(id).expect("every preview is drawn")).collect();
    let far = images.iter().map(|image| image.uv[2].max(image.uv[3])).fold(0.0, f32::max);
    assert!(far <= 1.0 && far > 0.5, "in a bigger atlas, where 256 pixels is an eighth: {far}");
    assert!(images.iter().all(|image| image.uv[2] - image.uv[0] == 0.125), "each a 256-pixel cell of 2048");
}
