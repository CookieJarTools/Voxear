//! Previews of a project's assets, drawn on a real GPU offscreen: what
//! the editor shows and scripts ask for through `Voxear.Preview`. Each test
//! skips itself without a usable Vulkan device.

use voxear_ecs::World;
use voxear_engine::{AssetPreviews, Content, ScriptPreviews};
use voxear_render::{Camera, PreviewOptions, Renderer, RendererSettings};
use voxear_scene::Transform;

fn hello() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello")
}

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(320, 180, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn frames(renderer: &mut Renderer, count: usize) {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    for frame in 0..count {
        renderer.render(&world, frame as f32 / 60.0).unwrap();
    }
}

#[test]
fn prefabs_models_and_meshes_have_previews_and_scenes_have_none() {
    let Some(mut renderer) = renderer() else { return };
    let mut content = Content::open(&hello()).unwrap();
    let mut previews = AssetPreviews::default();
    for asset in ["Prefabs/Lamp.prefab", "Models/Pyramid.gltf", "builtin:sphere"] {
        assert!(previews.image(&mut content, &mut renderer, asset, 64).is_none(), "{asset}: drawn next frame");
    }
    assert!(previews.image(&mut content, &mut renderer, "Scenes/Day.scene", 64).is_none());
    frames(&mut renderer, 1);
    for asset in ["Prefabs/Lamp.prefab", "Models/Pyramid.gltf", "builtin:sphere"] {
        let image =
            previews.image(&mut content, &mut renderer, asset, 64).unwrap_or_else(|| panic!("{asset} is drawn"));
        assert_eq!(image.size, 64);
    }
    assert!(previews.image(&mut content, &mut renderer, "Scenes/Day.scene", 64).is_none(), "a scene shows nothing");
    assert!(previews.has("Scenes/Day.scene", 64), "and is not tried again");
}

#[test]
fn a_scripts_preview_saves_as_a_png_once_drawn() {
    let Some(mut renderer) = renderer() else { return };
    let mut content = Content::open(&hello()).unwrap();
    let mut previews = ScriptPreviews::default();
    let options = PreviewOptions { size: 96, background: [0.1, 0.1, 0.12, 1.0], ..PreviewOptions::default() };
    let id = previews.create(&mut content, &mut renderer, "Prefabs/Firefly.prefab", options).unwrap();
    assert!(
        previews.create(&mut content, &mut renderer, "Scenes/Day.scene", options).is_err(),
        "a scene draws nothing"
    );
    let file = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("firefly-preview.png");
    let _ = std::fs::remove_file(&file);
    previews.save(&mut renderer, id, &file).unwrap();
    for _ in 0..6 {
        frames(&mut renderer, 1);
        previews.finish_saves(&mut renderer);
    }
    let bytes = std::fs::read(&file).expect("the PNG was written");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    // The header chunk follows the signature: its width and height, then 8 bits of RGBA (colour type 6).
    let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!((word(16), word(20), bytes[24], bytes[25]), (96, 96, 8, 6));
    assert!(previews.image(&mut renderer, id).is_some_and(|drawn| drawn.is_some()));
    previews.free(&mut renderer, id);
    assert!(previews.image(&mut renderer, id).is_none(), "let go");
}
