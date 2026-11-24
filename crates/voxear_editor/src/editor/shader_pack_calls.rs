//! Shader packs in the editor: worn in the Scene view from the Lighting
//! page or a pack's Inspector page, drawn again as their files are saved,
//! and for tools, `vox editor call` and `vox mcp`: `shader_pack.list`,
//! `shader_pack.wear` and `shader_pack.options`.

use serde_json::{Value as Json, json};
use voxear_engine::shader_packs;
use voxear_render::shader_packs::OptionKind;

use super::{
    Editor,
    link_calls::{Called, done},
};
use crate::{
    link::{bad, need},
    shader_pack_inspector::PackChange,
};

impl Editor {
    /// Wears the pack `reference` names in the Scene view, compiling on a
    /// thread and watching its files, or with none takes it off.
    pub(super) fn wear_shader_pack(&mut self, reference: Option<&str>) -> anyhow::Result<()> {
        let game = &mut self.game;
        shader_packs::wear_soon(&mut game.engine.world, &mut game.content, reference.unwrap_or_default(), true)
    }

    /// What a pack's Inspector page asked for.
    pub(super) fn shader_pack_change(&mut self, change: PackChange) {
        let done = match change {
            PackChange::Wear(reference, on) => self.wear_shader_pack(on.then_some(reference.as_str())),
            PackChange::Option(key, value) => {
                shader_packs::set_option(&mut self.game.engine.world, &key, value).map(|_| ())
            }
        };
        if let Err(error) = done {
            tracing::error!("{error:#}");
        }
        self.refresh_lighting_page();
    }

    /// Every pack there is to wear, and the one worn.
    pub(super) fn shader_pack_list(&mut self) -> Called {
        let game = &mut self.game;
        let packs = shader_packs::list(&mut game.content, None);
        done(json!({ "packs": packs, "worn": shader_packs::worn_json(&game.engine.world) }))
    }

    /// Wears a pack, compiled before this answers, and watches its files; or
    /// with none, or null, takes it off.
    pub(super) fn shader_pack_wear(&mut self, args: &Json) -> Called {
        let reference = match args.get("pack") {
            None | Some(Json::Null) => "",
            Some(Json::String(text)) => text.as_str(),
            Some(other) => return Err(bad(format!("`pack` is text or null, not {other}"))),
        };
        let game = &mut self.game;
        let world = &mut game.engine.world;
        shader_packs::wear(world, &mut game.content, reference).map_err(|error| bad(format!("{error:#}")))?;
        shader_packs::watch(world, true);
        self.refresh_lighting_page();
        done(json!({ "worn": shader_packs::worn_json(&self.game.engine.world) }))
    }

    /// The pack worn's passes and options, after setting one when `option`
    /// and `value` say.
    pub(super) fn shader_pack_options(&mut self, args: &Json) -> Called {
        let world = &mut self.game.engine.world;
        if let Some(key) = args.get("option").and_then(Json::as_str) {
            let options = shader_packs::options(world);
            let Some((option, _)) = options.iter().find(|(option, _)| option.key == key) else {
                let keys: Vec<&str> = options.iter().map(|(option, _)| option.key.as_str()).collect();
                return Err(bad(format!("no option `{key}`; the pack worn has {}", keys.join(", "))));
            };
            let value = match (need(args, "value")?, &option.kind) {
                (Json::Bool(on), _) => f32::from(u8::from(*on)),
                (Json::Number(number), _) => number.as_f64().unwrap_or_default() as f32,
                (Json::String(choice), OptionKind::Choice { .. }) => shader_packs::choice_value(option, choice)
                    .ok_or_else(|| bad(format!("`{choice}` is not one of `{key}`'s choices")))?,
                (Json::String(color), OptionKind::Color { .. }) => shader_packs::color_text_value(option, color)
                    .ok_or_else(|| bad(format!("`{color}` is not a colour: write `#rrggbb`, or `r,g,b` from 0 to 1")))?,
                (Json::Array(channels), OptionKind::Color { .. }) => {
                    let srgb: Option<Vec<f32>> = channels.iter().map(|c| c.as_f64().map(|c| c as f32)).collect();
                    let srgb = srgb.and_then(|srgb| <[f32; 3]>::try_from(srgb).ok());
                    let srgb = srgb.ok_or_else(|| bad(format!("`{key}` takes three numbers from 0 to 1")))?;
                    voxear_render::shader_packs::color_value(srgb)
                }
                (other, _) => return Err(bad(format!("{other} is not a value for `{key}`"))),
            };
            shader_packs::set_option(world, key, value).map_err(|error| bad(format!("{error:#}")))?;
        }
        done(json!({ "worn": shader_packs::worn_json(&self.game.engine.world) }))
    }
}
