//! Prompt Icons: a game's own pictures for its controls, shown by prompts
//! in place of the glyphs Voxear draws, for the device the player used
//! last; and, with a GPU, a set the input actions name loaded and its
//! pictures uploaded, a part of a sheet among them.

use voxear_engine::{
    Content,
    game_ui::GameUi,
    keymaps::prompt_glyphs_with,
    prompt_icons::{DRAWN, IconDevice, PromptIcons, PromptIconsFile, PromptSprites, SHEET, default_set},
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
fn voxears_own_set_has_a_picture_for_every_control_it_binds() {
    let set = default_set();
    let sprites = PromptSprites::resolve(&set, |texture| (texture == SHEET).then_some((7, [1024, 832])));
    for path in ["Keyboard/Space", "Keyboard/Q", "Keyboard/F12", "Keyboard/ArrowUp", "Mouse/Left", "Mouse/Delta"] {
        assert!(sprites.get(path, Family::Xbox).is_some(), "{path}");
    }
    for family in [Family::Xbox, Family::PlayStation, Family::Switch, Family::Steam] {
        for control in
            ["South", "East", "LeftShoulder", "RightTrigger", "Start", "DpadUp", "LeftStick", "RightStickPress"]
        {
            assert!(sprites.get(&format!("Gamepad/{control}"), family).is_some(), "{control} on {family:?}");
        }
    }
    // Each section's every control on the sheet, a whole cell of it.
    for device in IconDevice::ALL {
        for sprite in set.sprites(device).values() {
            let [x, y, w, h] = sprite.rect.expect("a part of the sheet");
            assert!(x + w <= 1024 && y + h <= 832 && (w, h) == (64, 64));
        }
    }
}

#[test]
fn a_key_typed_differently_from_a_us_keyboard_keeps_its_drawn_letter() {
    const QUIT: &str =
        "format = 1\n[[map]]\nname = \"Player\"\n[[map.action]]\nname = \"Quit\"\nbindings = [\"Keyboard/Q\"]\n";
    let mut input = Input::without_gamepads();
    input.load_actions(QUIT).unwrap();
    let sprites = PromptSprites::resolve(&default_set(), |texture| (texture == SHEET).then_some((7, [1024, 832])));
    assert!(prompt_glyphs_with(&input, "Quit", Some(&sprites))[0].image.is_some(), "a US keyboard's Q: its picture");
    input.devices_mut().set_typed("Q", "a");
    let quit = prompt_glyphs_with(&input, "Quit", Some(&sprites));
    assert_eq!((quit[0].image, quit[0].text.as_str()), (None, "A"), "a French keyboard's A, drawn");
}

#[test]
fn a_picture_for_ui_is_multiplied_by_its_alpha_in_linear_light() {
    let mut picture =
        image::RgbaImage::from_raw(3, 1, vec![0, 200, 0, 0, 255, 255, 255, 128, 10, 20, 30, 255]).unwrap();
    voxear_engine::prompt_icons::premultiply(&mut picture);
    assert_eq!(picture.get_pixel(0, 0).0, [0, 0, 0, 0], "see-through shows nothing of its colour");
    assert_eq!(picture.get_pixel(1, 0).0, [188, 188, 188, 128], "half white: half its light, as sRGB stores it");
    assert_eq!(picture.get_pixel(2, 0).0, [10, 20, 30, 255], "solid: as it was");
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
    // What the set has no picture for, Voxear's own: C, and a PlayStation pad's cross.
    let c = sprites.get("Keyboard/C", Family::Xbox).expect("C, from Voxear's sheet");
    let cross = sprites.get("Gamepad/South", Family::PlayStation).expect("a cross, from Voxear's sheet");
    assert_eq!(c.texture, cross.texture);
    assert!(c.texture != space.texture && c.texture != south.texture);

    // Naming none: Voxear's set alone; naming Drawn: none at all.
    let mut own = Input::without_gamepads();
    own.load_actions(&ACTIONS.replace("prompt_icons = \"UI/Prompts.prompticons\"\n", "")).unwrap();
    icons.follow(&own, &mut content, &mut renderer);
    let sprites = icons.sprites().expect("Voxear's own");
    assert_eq!(sprites.get("Keyboard/Space", Family::Xbox).unwrap().texture, c.texture, "its sheet, uploaded once");
    let mut drawn = Input::without_gamepads();
    drawn.load_actions(&ACTIONS.replace("UI/Prompts.prompticons", DRAWN)).unwrap();
    icons.follow(&drawn, &mut content, &mut renderer);
    assert!(icons.sprites().is_none(), "drawn by Voxear");
}
