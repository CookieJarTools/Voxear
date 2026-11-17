//! An Obj's Model Renderer for scripts (`ModelRenderer` in Voxear.Core):
//! the model it draws, which a script can change for another, and the
//! colours it is tinted. Changing the model needs the content and the
//! renderer, which the game lends scripts while they run (see
//! [`set_model`]).

use anyhow::{Context, Result, ensure};
use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_render::{ModelRenderer, Pose, Renderer, TINTS};
use voxear_scripting::ComponentKind;

use crate::{
    animation::AnimatorState,
    content::Content,
    scene_file::{ModelData, ModelSource},
};

// What `values` reads or writes, as `ModelRenderer` in Voxear.Core numbers them. A tint's slot is the number's
// higher bytes.
const HAS: i32 = 0;
const REMOVE: i32 = 2;
const TINT: i32 = 3;
const LOD_BIAS: i32 = 4;
const PARTS: i32 = 5;

// What `texts` answers: the model, by GUID, and a model to draw in its place, which gives the Obj a Model Renderer
// when it has none.
const MODEL: i32 = 0;
const SET_MODEL: i32 = 1;

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::ModelRenderer, values);
    voxear_scripting::set_kind_texts(ComponentKind::ModelRenderer, texts);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if !world.is_alive(entity) {
        return None;
    }
    let has = world.has::<ModelRenderer>(entity) || world.has::<ModelSource>(entity);
    let (slot, value) = ((what >> 8) as usize, what & 0xff);
    match (value, written) {
        (HAS, None) => Some([f32::from(u8::from(has)), 0.0, 0.0, 0.0]),
        (REMOVE, Some(_)) => {
            remove(world, entity);
            Some([0.0; 4])
        }
        (TINT, None) if slot < TINTS => {
            let tint = match world.get::<ModelRenderer>(entity) {
                Some(drawn) => drawn.tints[slot],
                None => world.get::<ModelSource>(entity)?.0.tints()[slot],
            };
            Some([tint[0], tint[1], tint[2], 1.0])
        }
        (TINT, Some(color)) if slot < TINTS && has => {
            let color = [color[0], color[1], color[2]]
                .map(|channel| if channel.is_finite() { channel.clamp(0.0, 1.0) } else { 1.0 });
            if let Some(drawn) = world.get_mut::<ModelRenderer>(entity) {
                drawn.tints[slot] = color;
            }
            // Kept as written too, for a game with nothing to draw it and for the Obj's next model.
            if let Some(source) = world.get_mut::<ModelSource>(entity) {
                source.0.set_tint(slot, color);
            }
            Some([color[0], color[1], color[2], 1.0])
        }
        (LOD_BIAS, None) => {
            let bias = match world.get::<ModelRenderer>(entity) {
                Some(drawn) => drawn.lod_bias,
                None => world.get::<ModelSource>(entity)?.0.lod_bias,
            };
            Some([bias, 0.0, 0.0, 0.0])
        }
        (LOD_BIAS, Some([bias, ..])) if has && bias.is_finite() => {
            let bias = bias.clamp(-4.0, 4.0);
            if let Some(drawn) = world.get_mut::<ModelRenderer>(entity) {
                drawn.lod_bias = bias;
            }
            if let Some(source) = world.get_mut::<ModelSource>(entity) {
                source.0.lod_bias = bias;
            }
            Some([bias, 0.0, 0.0, 0.0])
        }
        (PARTS, None) => {
            Some([world.get::<ModelRenderer>(entity).map_or(0, |drawn| drawn.parts.len()) as f32, 0.0, 0.0, 0.0])
        }
        _ => None,
    }
}

fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    if !world.is_alive(entity) {
        return None;
    }
    match what {
        MODEL => Some(world.get::<ModelSource>(entity)?.0.model.clone()),
        SET_MODEL => match voxear_scripting::set_model(world, entity, text)? {
            Ok(()) => Some(String::new()),
            Err(error) => {
                tracing::error!("an Obj could not draw {text}: {error:#}");
                None
            }
        },
        _ => None,
    }
}

/// Takes an Obj's Model Renderer away, and how it was posed.
fn remove(world: &mut World, entity: Entity) {
    world.remove::<ModelRenderer>(entity);
    world.remove::<ModelSource>(entity);
    world.remove::<Pose>(entity);
    world.remove::<AnimatorState>(entity);
}

/// Has `entity` draw the model `reference` names, by GUID or project
/// path, in place of the one it draws, or as the first it draws. Its tints
/// and LOD bias stay as they were; what it changed of the last model's
/// parts goes, as they were another model's. Without a renderer, as on a
/// server, it keeps the model's skeleton and clips alone, for what
/// animates it.
pub fn set_model(
    world: &mut World,
    content: &mut Content,
    renderer: Option<&mut Renderer>,
    entity: Entity,
    reference: &str,
) -> Result<()> {
    ensure!(world.is_alive(entity), "the Obj is gone");
    let guid = content.parse(reference)?.guid().with_context(|| format!("{reference} is not an asset"))?;
    let kind = content.db().get(guid).map(|entry| entry.kind);
    ensure!(kind == Some(AssetKind::Model), "{reference} is not a model");
    let mut data = match world.get::<ModelSource>(entity) {
        Some(source) => ModelData { model: guid.to_string(), parts: Vec::new(), ..source.0.clone() },
        None => ModelData::of(guid.to_string()),
    };
    if let Some(drawn) = world.get::<ModelRenderer>(entity) {
        data.lod_bias = drawn.lod_bias;
        for (slot, tint) in drawn.tints.iter().enumerate() {
            data.set_tint(slot, *tint);
        }
    }
    let kept = world.get::<ModelRenderer>(entity).map(|drawn| drawn.tints);
    let drawn = match renderer {
        Some(renderer) => Some(crate::prefab::model_renderer(content, renderer, &data)?),
        None => crate::prefab::animated(content, &data),
    };
    let bones = |drawn: Option<&ModelRenderer>| {
        drawn
            .and_then(|drawn| drawn.animation.as_ref())
            .map(|animation| animation.skeleton.bones.iter().map(|bone| bone.name.clone()).collect::<Vec<_>>())
    };
    // A pose of other bones would move the new skin wrongly; what animates it poses it afresh. The same bones, as
    // two parts made on one rig have, keep their pose, so the change shows no step.
    let other_bones = bones(world.get::<ModelRenderer>(entity)) != bones(drawn.as_ref());
    match drawn {
        Some(drawn) => drop(world.insert(entity, ModelRenderer { tints: kept.unwrap_or(drawn.tints), ..drawn })),
        None => drop(world.remove::<ModelRenderer>(entity)),
    }
    world.insert(entity, ModelSource(data));
    if other_bones {
        world.remove::<Pose>(entity);
        world.remove::<AnimatorState>(entity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scripts_tint_is_kept_on_the_renderer_and_as_written() {
        let mut world = World::new();
        let drawn = world.spawn((ModelRenderer::new(Vec::new()), ModelSource(ModelData::of("Models/Hair.boxmodel"))));
        let bare = world.spawn(());
        assert_eq!(values(&mut world, drawn, HAS, None), Some([1.0, 0.0, 0.0, 0.0]));
        assert_eq!(values(&mut world, bare, HAS, None), Some([0.0; 4]));
        assert_eq!(values(&mut world, drawn, TINT, None), Some([1.0; 4]), "white, which changes nothing");

        assert_eq!(values(&mut world, drawn, TINT, Some([0.4, 0.2, 2.0, 1.0])), Some([0.4, 0.2, 1.0, 1.0]));
        assert_eq!(world.get::<ModelRenderer>(drawn).unwrap().tints[0], [0.4, 0.2, 1.0]);
        assert_eq!(world.get::<ModelSource>(drawn).unwrap().0.tint, [0.4, 0.2, 1.0]);
        assert_eq!(values(&mut world, bare, TINT, Some([0.4, 0.2, 0.1, 1.0])), None, "nothing to tint");

        assert_eq!(texts(&mut world, drawn, MODEL, ""), Some("Models/Hair.boxmodel".to_string()));
        values(&mut world, drawn, REMOVE, Some([0.0; 4]));
        assert!(!world.has::<ModelRenderer>(drawn) && !world.has::<ModelSource>(drawn));
    }
}
