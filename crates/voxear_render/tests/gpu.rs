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
    if cooked {
        model.compress_textures();
        model = voxear_render::PreparedModel::from_bytes("Quad.gltf", &model.to_bytes()).unwrap();
    }
    let imported = renderer.upload_model(&model).unwrap();
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    world.spawn((
        MeshRenderer { mesh: imported.nodes[0].mesh, material: imported.nodes[0].material },
        Transform::IDENTITY,
    ));
    run(&mut renderer, &world, &mut 0.0, 8);
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    Some((at(width / 2, height / 2), at(2, 2)))
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
