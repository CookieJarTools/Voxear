//! The open scene's lighting for tools, `vox editor call` and `vox mcp`:
//! `lighting.get` reads every setting in effect, as the Lighting page shows
//! them, and which upscalers run on this GPU; `lighting.set` changes one, as
//! the page does, written into the lighting settings' file as a step to undo.

use serde_json::{Value as Json, json};
use voxear_engine::local_link::ErrorKind;
use voxear_render::Renderer;

use super::{
    Editor,
    link_calls::{Called, done},
};
use crate::{
    lighting_panel::{self, LightingAction},
    link::{bad, need, need_text, refuse},
};

impl Editor {
    pub(super) fn lighting_get(&mut self, renderer: &Renderer) -> Called {
        let (named, file, values) =
            lighting_panel::scene_effective(&mut self.game).map_err(|e| bad(format!("{e:#}")))?;
        let upscalers: Vec<Json> = renderer
            .upscalers()
            .into_iter()
            .map(|(upscaler, why)| {
                let name = serde_json::to_value(upscaler).unwrap_or_default();
                json!({ "name": name, "runs": why.is_none(), "why": why })
            })
            .collect();
        done(json!({
            "lighting": named,
            "changeable": file.is_some(),
            "settings": serde_json::to_value(values).unwrap_or_default(),
            "paths": lighting_panel::setting_paths(),
            "upscalers": upscalers,
        }))
    }

    pub(super) fn lighting_set(&mut self, args: &Json, renderer: &mut Renderer) -> Called {
        let dotted = need_text(args, "path")?;
        let path = lighting_panel::setting_path(dotted)
            .ok_or_else(|| bad(format!("no setting `{dotted}`; `lighting.get` lists them under `paths`")))?;
        let (_, file, _) = lighting_panel::scene_effective(&mut self.game).map_err(|e| bad(format!("{e:#}")))?;
        let Some(file) = file else {
            return Err(refuse(
                ErrorKind::NotAllowed,
                "the scene is lit by built-in lighting or its own: make lighting settings of the project's first",
            ));
        };
        let value = match need(args, "value")? {
            Json::Null => None,
            value => Some(toml_value(value).ok_or_else(|| bad(format!("{value} is not a setting's value")))?),
        };
        self.lighting_action(LightingAction::Set { file, path, value }, renderer);
        self.lighting_get(renderer)
    }
}

/// A setting's value from JSON: a switch, a number, a word, or a list of
/// numbers.
fn toml_value(json: &Json) -> Option<toml_edit::Value> {
    Some(match json {
        Json::Bool(on) => (*on).into(),
        Json::Number(number) => match number.as_i64() {
            Some(whole) => whole.into(),
            None => number.as_f64()?.into(),
        },
        Json::String(text) => text.as_str().into(),
        Json::Array(items) => toml_edit::Value::Array(items.iter().map(|item| item.as_f64()).collect::<Option<_>>()?),
        _ => return None,
    })
}
