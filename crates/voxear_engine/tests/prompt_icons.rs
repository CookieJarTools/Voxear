//! Prompt Icons: a game's own pictures for its controls, shown by prompts
//! in place of the glyphs Voxear draws, for the device the player used
//! last; and, with a GPU, a set the input actions name loaded and its
//! pictures uploaded, a part of a sheet among them.

use voxear_engine::{
    Content,
    game_ui::GameUi,
    keymaps::prompt_glyphs_with,
    prompt_icons::{PromptIcons, PromptIconsFile, PromptSprites},
};
use voxear_input::{Family, Input, Pad};
use voxear_ui::{ElementKind, ImageRef};

const ACTIONS: &str = "format = 1\nprompt_icons = \"UI/Prompts.prompticons\"\n\n[[map]]\nname = \"Player\"\n\n\
                       [[map.action]]\nname = \"Jump\"\nbindings = [\"Keyboard/Space\", \"Gamepad/South\"]\n\n\
                       [[map.action]]\nname = \"Crouch\"\nbindings = [\"Keyboard/C\", \"Gamepad/East\"]\n";

const ICONS: &str = "format = 1\n\n[keyboard]\nSpace = { texture = \"UI/Space.png\" }\n\n\
                     [xbox]\nSouth = { texture = \"UI/Pads.png\", rect = [64, 0, 64, 32] }\n";

/// A set's pictures, each texture its number in a list, every one 128 by
/// 64 pixels.
fn sprites() -> PromptSprites {
    let file: PromptIconsFile = toml::from_str(ICONS).unwrap();
    let textures = ["UI/Space.png", "UI/Pads.png"];
    PromptSprites::resolve(&file, |texture| Some((textures.iter().position(|t| *t == texture)? as u32, [128, 64])))
}

#[test]
fn a_prompt_shows_the_sets_picture_for_a_control_it_has_one_for() {
    let mut input = Input::without_gamepads();
    input.load_actions(ACTIONS).unwrap();
    assert_eq!(input.prompt_icons(), Some("UI/Prompts.prompticons"));
    let sprites = sprites();

    // The keyboard: Space's picture, all of it; C, which the set has none for, as Voxear draws it.
    let jump = prompt_glyphs_with(&input, "Jump", Some(&sprites));
    assert_eq!(jump[0].image, Some(ImageRef { texture: 0, uv: [0.0, 0.0, 1.0, 1.0] }));
    let crouch = prompt_glyphs_with(&input, "Crouch", Some(&sprites));
    assert_eq!((crouch[0].image, crouch[0].text.as_str()), (None, "C"));

    // An Xbox pad: South's picture, the part of the sheet the set says.
    let mut pad = Pad { family: Family::Xbox, ..Pad::default() };
    pad.buttons.insert("East", 1.0);
    input.devices_mut().set_pad(0, pad);
    input.update(0.016);
    let jump = prompt_glyphs_with(&input, "Jump", Some(&sprites));
    assert_eq!(jump[0].image, Some(ImageRef { texture: 1, uv: [0.5, 0.0, 1.0, 0.5] }));

    // A PlayStation pad: the set has nothing for it, so its own glyph.
    let mut pad = Pad { family: Family::PlayStation, ..Pad::default() };
    pad.buttons.insert("East", 1.0);
    input.devices_mut().set_pad(0, pad);
    input.update(0.016);
    let jump = prompt_glyphs_with(&input, "Jump", Some(&sprites));
    assert_eq!(jump[0].image, None);
    assert!(jump[0].icon.is_some() || !jump[0].text.is_empty(), "drawn as Voxear draws a PlayStation pad's cross");
}

#[test]
fn a_prompt_on_the_screen_draws_the_picture() {
    let mut input = Input::without_gamepads();
    input.load_actions(ACTIONS).unwrap();
    let mut ui = GameUi::default();
    let document = ui.new_document();
    let root = ui.find(document, "").unwrap();
    ui.create(root, ElementKind::Prompt, "Jump").unwrap();
    ui.follow_input(&input, Some(&sprites()));
    let screen = ui.ui();
    let mut sprites = Vec::new();
    let mut stack = vec![screen.root()];
    while let Some(node) = stack.pop() {
        if screen.has_class(node, "prompt-sprite") {
            sprites.push(node);
        }
        stack.extend(screen.children(node));
    }
    assert_eq!(sprites.len(), 1, "Space's picture, in place of its keycap");

    // Without the set, the keycap again.
    ui.follow_input(&input, None);
    let screen = ui.ui();
    let mut keys = 0;
    let mut stack = vec![screen.root()];
    while let Some(node) = stack.pop() {
        keys += usize::from(screen.has_class(node, "prompt-glyph") && screen.has_class(node, "key"));
        assert!(!screen.has_class(node, "prompt-sprite"));
        stack.extend(screen.children(node));
    }
    assert_eq!(keys, 1);
}

#[test]
fn the_set_the_actions_name_is_loaded_and_its_pictures_uploaded() {
    let renderer = voxear_render::Renderer::new_offscreen(64, 64, &voxear_render::RendererSettings::default());
    let Ok(mut renderer) = renderer else { return eprintln!("skipping: no usable GPU") };
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("UI")).unwrap();
    let picture = |name: &str, width: u32, height: u32| {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
        image.save(dir.path().join(name)).unwrap();
    };
    picture("UI/Space.png", 64, 64);
    picture("UI/Pads.png", 128, 64);
    std::fs::write(dir.path().join("UI/Prompts.prompticons"), ICONS).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let mut input = Input::without_gamepads();
    input.load_actions(ACTIONS).unwrap();

    let mut icons = PromptIcons::default();
    icons.follow(&input, &mut content, &mut renderer);
    let sprites = icons.sprites().expect("the set the actions name");
    let space = sprites.get("Keyboard/Space", Family::Xbox).expect("Space's picture, uploaded");
    assert_eq!(space.uv, [0.0, 0.0, 1.0, 1.0]);
    let south = sprites.get("Gamepad/South", Family::Xbox).expect("South's, from the sheet");
    assert_eq!(south.uv, [0.5, 0.0, 1.0, 0.5]);
    assert_ne!(space.texture, south.texture);
}
