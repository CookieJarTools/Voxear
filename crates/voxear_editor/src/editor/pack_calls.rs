//! What a game's options menu changes, for tools through `vox editor call`
//! and `vox mcp`: the look pack and the texture pack worn, as the Lighting
//! page's menus wear them (`look_pack.list`, `look_pack.wear`,
//! `texture_pack.list`, `texture_pack.wear`); a look pack's own settings,
//! its look shader's among them, as its Inspector page writes them
//! (`look_pack.get`, `look_pack.set`); and while the game plays, its
//! volumes as `Audio` sets them (`audio.mix`) and its display as `Screen`
//! does (`display`).

use serde_json::{Value as Json, json};
use voxear_render::display::{Encoding, HdrChoice};
use voxear_scripting::{AudioAsk, BusValue};

use super::{
    Editor,
    link_calls::{Called, done},
};
use crate::link::{bad, opt_bool, opt_text};

/// The packs' references as calls take them: text, or null for none.
fn pack_named(args: &Json) -> Result<&str, voxear_engine::local_link::LinkError> {
    match args.get("pack") {
        None | Some(Json::Null) => Ok(""),
        Some(Json::String(text)) => Ok(text.as_str()),
        Some(other) => Err(bad(format!("`pack` is text or null, not {other}"))),
    }
}

const HDR: [(HdrChoice, &str); 4] =
    [(HdrChoice::Off, "off"), (HdrChoice::On, "on"), (HdrChoice::Hdr10, "hdr10"), (HdrChoice::ScRgb, "scrgb")];

impl Editor {
    /// Every look pack there is to wear, and the one the Scene view wears.
    pub(super) fn look_pack_list(&mut self) -> Called {
        let game = &mut self.game;
        let packs = voxear_engine::look_packs::list(&mut game.content, None);
        done(json!({ "packs": packs, "worn": voxear_engine::look_packs::worn(&game.engine.world) }))
    }

    /// Wears a look pack, or with none takes it off.
    pub(super) fn look_pack_wear(&mut self, args: &Json) -> Called {
        let reference = pack_named(args)?;
        let game = &mut self.game;
        voxear_engine::look_packs::wear(&mut game.engine.world, &mut game.content, reference)
            .map_err(|error| bad(format!("{error:#}")))?;
        self.refresh_lighting_page();
        done(json!({ "worn": voxear_engine::look_packs::worn(&self.game.engine.world) }))
    }

    /// A look pack of the project's as its file writes it, the paths
    /// `look_pack.set` takes, and the project's look shaders with their
    /// properties, for its `[shader]`.
    pub(super) fn look_pack_get(&mut self, args: &Json) -> Called {
        let (path, file) = self.look_pack_file(args)?;
        let text = std::fs::read_to_string(&file).map_err(|error| bad(format!("reading {path}: {error}")))?;
        let shaders: Vec<Json> = self
            .game
            .content
            .shaders()
            .shaders
            .iter()
            .filter_map(|shader| Some((shader, shader.info.as_ref().ok().filter(|info| info.look)?)))
            .map(|(shader, info)| {
                let properties: Vec<Json> = info
                    .properties
                    .iter()
                    .map(|property| json!({ "name": property.name, "kind": format!("{:?}", property.kind) }))
                    .collect();
                json!({ "shader": shader.path, "properties": properties })
            })
            .collect();
        done(json!({
            "pack": path,
            "settings": crate::link::toml_json(&text),
            "paths": crate::lighting_panel::pack_setting_paths(),
            "shaders": shaders,
        }))
    }

    /// Writes one setting of a look pack of the project's, as its Inspector
    /// page does, as a step to undo; null takes it out, to the level's.
    pub(super) fn look_pack_set(&mut self, args: &Json) -> Called {
        let (_, file) = self.look_pack_file(args)?;
        let dotted = crate::link::need_text(args, "path")?;
        let path: Vec<&str> = dotted.split('.').collect();
        let given = args.get("value").ok_or_else(|| bad("`value` is missing: a setting's value, or null"))?;
        let value = match given {
            Json::Null => None,
            value => Some(
                super::lighting_calls::toml_value(value)
                    .ok_or_else(|| bad(format!("{value} is not a setting's value")))?,
            ),
        };
        self.set_pack_setting(&file, &path, value).map_err(|error| bad(format!("{error:#}")))?;
        self.refresh_lighting_page();
        self.look_pack_get(args)
    }

    /// The look pack `pack` names: its project path and its file.
    fn look_pack_file(
        &self,
        args: &Json,
    ) -> Result<(String, std::path::PathBuf), voxear_engine::local_link::LinkError> {
        let (_, path, kind) = crate::link::resolve_project_asset(&self.game, crate::link::need(args, "pack")?)?;
        if kind != voxear_assets::AssetKind::LookPack {
            return Err(bad(format!("{path} is not a look pack")));
        }
        Ok((path.clone(), self.game.project.root.join(&path)))
    }

    /// Every texture pack there is to wear, and the one worn.
    pub(super) fn texture_pack_list(&mut self) -> Called {
        let content = &mut self.game.content;
        let packs = voxear_engine::texture_packs::list(content, None);
        done(json!({ "packs": packs, "worn": voxear_engine::texture_packs::worn(content) }))
    }

    /// Wears a texture pack, or with none takes it off.
    pub(super) fn texture_pack_wear(&mut self, args: &Json) -> Called {
        let reference = pack_named(args)?;
        let content = &mut self.game.content;
        voxear_engine::texture_packs::wear(content, reference).map_err(|error| bad(format!("{error:#}")))?;
        self.refresh_lighting_page();
        done(json!({ "worn": voxear_engine::texture_packs::worn(&self.game.content) }))
    }

    /// The game's volumes, headphones and reverb, after setting those given
    /// while it plays, as `Audio` in a script does.
    pub(super) fn audio_mix(&mut self, args: &Json) -> Called {
        let numbers = [("master", BusValue::Master), ("music", BusValue::Music), ("effects", BusValue::Effects)];
        let switches = [("headphones", BusValue::Headphones), ("reverb", BusValue::Reverb)];
        let mut sets = Vec::new();
        for (key, bus) in numbers {
            if let Some(value) = args.get(key).filter(|value| !value.is_null()) {
                let volume = value.as_f64().ok_or_else(|| bad(format!("`{key}` is a volume, from 0 to 1")))?;
                sets.push((bus, (volume as f32).clamp(0.0, 1.0)));
            }
        }
        for (key, bus) in switches {
            if let Some(on) = opt_bool(args, key) {
                sets.push((bus, f32::from(u8::from(on))));
            }
        }
        if !sets.is_empty() {
            self.refuse_unless_playing("change the game's volumes")?;
        }
        let game = &mut self.game;
        for (bus, value) in sets {
            game.audio.answer(&mut game.content, None, AudioAsk::Bus(bus), Some(value));
        }
        let mut read = |bus| game.audio.answer(&mut game.content, None, AudioAsk::Bus(bus), None).unwrap_or(f32::NAN);
        let (master, music, effects) = (read(BusValue::Master), read(BusValue::Music), read(BusValue::Effects));
        let (headphones, reverb) = (read(BusValue::Headphones) == 1.0, read(BusValue::Reverb) == 1.0);
        done(
            json!({ "master": master, "music": music, "effects": effects, "headphones": headphones, "reverb": reverb }),
        )
    }

    /// The game's window and its HDR output, after setting the HDR asked
    /// for while it plays, as `Screen.SetHdr` in a script does: made so when
    /// the game shows in a window of its own.
    pub(super) fn display(&mut self, args: &Json) -> Called {
        let mut asked = self.game.display.hdr;
        let mut changed = false;
        if let Some(mode) = opt_text(args, "hdr") {
            asked.hdr = HDR
                .iter()
                .find(|(_, name)| *name == mode)
                .map(|(choice, _)| *choice)
                .ok_or_else(|| bad(format!("`{mode}` is not an HDR mode: off, on, hdr10 or scrgb")))?;
            changed = true;
        }
        for (key, value) in [("paper_white", &mut asked.paper_white), ("peak", &mut asked.peak)] {
            if let Some(nits) = args.get(key).filter(|nits| !nits.is_null()) {
                *value = nits.as_f64().ok_or_else(|| bad(format!("`{key}` is a number of nits")))? as f32;
                changed = true;
            }
        }
        if changed {
            self.refuse_unless_playing("change the game's display")?;
            let display = &mut self.game.display;
            let held = asked.held();
            display.hdr_changed |= held != display.hdr;
            display.hdr = held;
        }
        let display = &self.game.display;
        let name = |choice: HdrChoice| HDR.iter().find(|(c, _)| *c == choice).map_or("off", |(_, name)| name);
        let output = match display.hdr_shown.encoding {
            Encoding::Sdr => "sdr",
            Encoding::Hdr10 => "hdr10",
            Encoding::ScRgb => "scrgb",
        };
        let offered = display.hdr_shown.offered;
        done(json!({
            "width": display.width,
            "height": display.height,
            "fullscreen": display.fullscreen,
            "hdr": name(display.hdr.hdr),
            "paper_white": display.hdr.paper_white,
            "peak": display.hdr.peak,
            "output": output,
            "offered": { "hdr10": offered.hdr10, "scrgb": offered.scrgb },
        }))
    }
}
