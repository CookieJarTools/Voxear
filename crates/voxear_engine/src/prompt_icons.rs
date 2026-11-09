//! Prompt Icons: a game's own pictures for its controls, which prompts show
//! in place of the glyphs Voxear draws. A `.prompticons` file lists, for
//! each kind of device, the picture for each control by its name, as its
//! path in a binding names it: `Space` for `Keyboard/Space`, `South` for
//! `Gamepad/South`. A picture is a texture, or a part of one, as a sheet of
//! icons holds many:
//!
//! ```toml
//! format = 1
//!
//! [keyboard]
//! Space = { texture = "UI/Prompts/Space.png" }
//! Enter = { texture = "UI/Prompts/Keys.png", rect = [64, 0, 64, 64] }
//!
//! [xbox]
//! South = { texture = "UI/Prompts/Xbox/A.png" }
//! ```
//!
//! A pad's controls come from the section for the kind of pad the player
//! holds: `xbox`, `playstation`, `switch` or `steam`. The project's input
//! actions name the set prompts use (`prompt_icons`). A control the set has
//! no picture for is drawn as Voxear draws it.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use voxear_input::{Family, Input};
use voxear_render::Renderer;
use voxear_ui::ImageRef;

use crate::Content;

/// A `.prompticons` file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptIconsFile {
    #[serde(default = "format")]
    pub format: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keyboard: BTreeMap<String, Sprite>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mouse: BTreeMap<String, Sprite>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub xbox: BTreeMap<String, Sprite>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub playstation: BTreeMap<String, Sprite>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub switch: BTreeMap<String, Sprite>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub steam: BTreeMap<String, Sprite>,
}

fn format() -> u32 {
    1
}

/// A control's picture: a texture by GUID or project path, and the part of
/// it to show, in its pixels from its top left (left, top, width, height),
/// or all of it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    pub texture: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<[u32; 4]>,
}

/// The kinds of device a set has pictures for, each a section of its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IconDevice {
    Keyboard,
    Mouse,
    Xbox,
    PlayStation,
    Switch,
    Steam,
}

impl IconDevice {
    pub const ALL: [IconDevice; 6] = [
        IconDevice::Keyboard,
        IconDevice::Mouse,
        IconDevice::Xbox,
        IconDevice::PlayStation,
        IconDevice::Switch,
        IconDevice::Steam,
    ];

    /// How the Inspector names it.
    pub fn title(self) -> &'static str {
        match self {
            IconDevice::Keyboard => "Keyboard",
            IconDevice::Mouse => "Mouse",
            IconDevice::Xbox => "Xbox",
            IconDevice::PlayStation => "PlayStation",
            IconDevice::Switch => "Switch",
            IconDevice::Steam => "Steam",
        }
    }

    /// The device a control's path is on, and the control's name there: a
    /// pad's for the kind of pad the player holds.
    pub fn of(path: &str, family: Family) -> Option<(IconDevice, &str)> {
        let (device, control) = path.split_once('/')?;
        let device = match device {
            "Keyboard" => IconDevice::Keyboard,
            "Mouse" => IconDevice::Mouse,
            "Gamepad" => match family {
                Family::Xbox => IconDevice::Xbox,
                Family::PlayStation => IconDevice::PlayStation,
                Family::Switch => IconDevice::Switch,
                Family::Steam => IconDevice::Steam,
            },
            _ => return None,
        };
        Some((device, control))
    }
}

impl PromptIconsFile {
    /// The pictures for one kind of device, by control.
    pub fn sprites(&self, device: IconDevice) -> &BTreeMap<String, Sprite> {
        match device {
            IconDevice::Keyboard => &self.keyboard,
            IconDevice::Mouse => &self.mouse,
            IconDevice::Xbox => &self.xbox,
            IconDevice::PlayStation => &self.playstation,
            IconDevice::Switch => &self.switch,
            IconDevice::Steam => &self.steam,
        }
    }

    pub fn sprites_mut(&mut self, device: IconDevice) -> &mut BTreeMap<String, Sprite> {
        match device {
            IconDevice::Keyboard => &mut self.keyboard,
            IconDevice::Mouse => &mut self.mouse,
            IconDevice::Xbox => &mut self.xbox,
            IconDevice::PlayStation => &mut self.playstation,
            IconDevice::Switch => &mut self.switch,
            IconDevice::Steam => &mut self.steam,
        }
    }

    /// The file as the editor writes it, with a line saying what it is.
    pub fn to_text(&self) -> Result<String> {
        let text = toml::to_string(self).context("writing the Prompt Icons")?;
        Ok(format!("# Voxear Prompt Icons: the pictures prompts show for controls, by device.\n{text}"))
    }
}

/// A set's pictures, each a texture uploaded and the part of it to draw.
#[derive(Clone, Debug, Default)]
pub struct PromptSprites {
    sprites: HashMap<(IconDevice, String), ImageRef>,
}

impl PromptSprites {
    /// The set's pictures, each texture loaded by `load`, which gives a
    /// texture's number and its size in pixels, or none when it cannot be
    /// read: those are left out, to be drawn as Voxear draws them.
    pub fn resolve(file: &PromptIconsFile, mut load: impl FnMut(&str) -> Option<(u32, [u32; 2])>) -> Self {
        let mut sprites = HashMap::new();
        for device in IconDevice::ALL {
            for (control, sprite) in file.sprites(device) {
                let Some((texture, [width, height])) = load(&sprite.texture) else { continue };
                let (width, height) = (width.max(1) as f32, height.max(1) as f32);
                let uv = match sprite.rect {
                    Some([x, y, w, h]) => {
                        let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
                        [x / width, y / height, (x + w) / width, (y + h) / height]
                    }
                    None => [0.0, 0.0, 1.0, 1.0],
                };
                sprites.insert((device, control.clone()), ImageRef { texture, uv });
            }
        }
        Self { sprites }
    }

    /// The picture for a control by its path, for a pad of `family`.
    pub fn get(&self, path: &str, family: Family) -> Option<ImageRef> {
        let (device, control) = IconDevice::of(path, family)?;
        self.sprites.get(&(device, control.to_string())).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }
}

/// The set the game's prompts use: read and its textures uploaded once the
/// input actions name it and a renderer is there, and again when told it
/// changed.
#[derive(Default)]
pub struct PromptIcons {
    /// The set, by the reference the actions gave, as last loaded.
    loaded: Option<Option<String>>,
    sprites: Option<PromptSprites>,
}

impl PromptIcons {
    /// The set's pictures, while there is one.
    pub fn sprites(&self) -> Option<&PromptSprites> {
        self.sprites.as_ref()
    }

    /// Reads the set again at the next frame, as after its file or a
    /// picture it names changed.
    pub fn forget(&mut self) {
        self.loaded = None;
    }

    /// Loads the set `input`'s actions name, when it is not the one loaded:
    /// needs a renderer, to upload its pictures.
    pub fn follow(&mut self, input: &Input, content: &mut Content, renderer: &mut Renderer) {
        let wanted = input.prompt_icons().map(str::to_string);
        if self.loaded.as_ref() == Some(&wanted) {
            return;
        }
        self.loaded = Some(wanted.clone());
        self.sprites = None;
        let Some(reference) = wanted else { return };
        match read(content, &reference) {
            Ok(file) => {
                let sprites = PromptSprites::resolve(&file, |texture| {
                    let guid = content.parse(texture).ok()?.guid()?;
                    match content.ui_picture(renderer, guid) {
                        Ok((texture, size)) => Some((texture.0, size)),
                        Err(error) => {
                            tracing::warn!("{reference}: {error:#}");
                            None
                        }
                    }
                });
                self.sprites = Some(sprites);
            }
            Err(error) => tracing::error!("the input actions' prompt icons: {error:#}"),
        }
    }
}

/// A `.prompticons` file, by GUID or project path.
pub fn read(content: &mut Content, reference: &str) -> Result<PromptIconsFile> {
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not Prompt Icons"))?;
    let (text, path) = content.asset_text(guid, voxear_assets::AssetKind::PromptIcons)?;
    toml::from_str(&text).with_context(|| format!("reading {path}"))
}
