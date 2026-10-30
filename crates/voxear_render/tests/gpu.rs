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
    use voxear_ui::{Key, Modifiers, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <Panel style="position: absolute; left: 16px; top: 16px; width: 280px; padding: 12px; gap: 10px;
                          background-color: #111216; border-radius: 10px; font-size: 12px; color: #c8c9d0">
                <TextField name="name" placeholder="Name" value="Firefly spawner" />
                <TextField placeholder="Search assets" />
                <Toggle name="fly" value="true">Can fly</Toggle>
                <Toggle>Snap to grid</Toggle>
                <Slider name="speed" min="0" max="20" value="6" />
                <Foldout text="Movement">
                    <Label>Speed</Label>
                    <Label>Lift force</Label>
                </Foldout>
                <Button>Bake path</Button>
            </Panel>
        </vui>"#;
    let document = ui.load_markup(root, "Form.vui", markup, &mut |_| Err("no files".into())).unwrap();
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
    let fly = ui.find(document, "fly").unwrap();
    let check_box = ui.children(fly)[0];
    let [x, y, w, _] = ui.rect(check_box);
    assert_eq!(at(x + 2.0, y + 2.0), [0x8b, 0x93, 0xff], "a ticked toggle fills with the accent");
    let [x, y, _, h] = ui.rect(name);
    assert_eq!(at(x, y + h / 2.0)[2], 0xff, "the focused field's border is the accent");
    assert!(w > 0.0 && ui.take_warnings().is_empty());
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
    assert_eq!(pixels[i..i + 3], [0x2a, 0x2b, 0x30], "the divider between the panes");
    assert!(ui.take_warnings().is_empty());
}
