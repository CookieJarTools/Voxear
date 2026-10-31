//! Previews of assets: what a preview of a material, prefab, model or mesh
//! shows, as parts for the renderer to draw (see
//! [`Renderer::add_preview`]), and [`AssetPreviews`], which keeps one per
//! asset and size, as the editor's panels show them.
//!
//! A material shows on a sphere; a prefab as all its Objs' meshes where
//! they sit in it; a model as all its nodes; and a mesh on its own, in the
//! default material. The easy way to a preview is
//! [`AssetPreviews::image`]; the advanced one is [`preview_parts`] and
//! [`Renderer::add_preview`] with [`PreviewOptions`] of one's own: any
//! background, how the object is turned and moved, zoom, field of view and
//! light.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use voxear_assets::{AssetKind, AssetRef};
use voxear_core::math::Mat4;
use voxear_render::{MeshRenderer, PreviewId, PreviewImage, PreviewOptions, PreviewPart, Primitive, Renderer};

use crate::{
    content::{Content, MaterialSource},
    scene_file::default_material,
};

/// What a preview of `reference` shows: an asset's path or GUID, or a mesh
/// reference such as `builtin:cube` or `<model>#mesh0.0`. Loads what it
/// needs; fails for what has nothing to show, such as a scene or a sound.
pub fn preview_parts(content: &mut Content, renderer: &mut Renderer, reference: &str) -> Result<Vec<PreviewPart>> {
    let parsed = content.parse(reference)?;
    let default = |content: &mut Content, renderer: &mut Renderer| {
        content.material(renderer, &MaterialSource::Inline(default_material()))
    };
    let (guid, part) = match &parsed {
        AssetRef::Builtin(_) => {
            let mesh = content.mesh(renderer, &parsed)?;
            let material = default(content, renderer)?;
            return Ok(vec![PreviewPart { mesh, material, transform: Mat4::IDENTITY }]);
        }
        AssetRef::Asset { guid, part } => (*guid, part.clone()),
    };
    let kind = content.db().get(guid).map(|entry| entry.kind).with_context(|| format!("no asset `{reference}`"))?;
    match kind {
        AssetKind::Model if part.is_some() => {
            let mesh = content.mesh(renderer, &parsed)?;
            let material = default(content, renderer)?;
            Ok(vec![PreviewPart { mesh, material, transform: Mat4::IDENTITY }])
        }
        AssetKind::Model => Ok(content
            .model(renderer, guid)?
            .iter()
            .map(|node| PreviewPart {
                mesh: node.mesh,
                material: node.material,
                transform: Mat4::from_scale_rotation_translation(node.scale, node.rotation, node.position),
            })
            .collect()),
        AssetKind::Material => {
            let mesh = renderer.primitive(Primitive::Sphere)?;
            let material = content.material(renderer, &MaterialSource::Asset(parsed))?;
            Ok(vec![PreviewPart { mesh, material, transform: Mat4::IDENTITY }])
        }
        AssetKind::Prefab => {
            let id = content.prefab(Some(&mut *renderer), reference)?;
            let template = content.loaded_template(Some(renderer), id)?;
            Ok(prefab_parts(
                &template.objs.iter().map(|o| (o.parent, o.transform.matrix(), o.renderer)).collect::<Vec<_>>(),
            ))
        }
        _ => bail!("`{reference}` has nothing to preview"),
    }
}

/// A prefab's drawn Objs, each placed by its parents' transforms and its
/// own: given each Obj's parent, local matrix and renderer.
fn prefab_parts(objs: &[(Option<usize>, Mat4, Option<MeshRenderer>)]) -> Vec<PreviewPart> {
    let world = |mut at: usize| {
        let mut matrix = Mat4::IDENTITY;
        // A parent chain no longer than the template, so a loop cannot spin.
        for _ in 0..=objs.len() {
            let (parent, local, _) = objs[at];
            matrix = local * matrix;
            match parent {
                Some(parent) => at = parent,
                None => break,
            }
        }
        matrix
    };
    objs.iter()
        .enumerate()
        .filter_map(|(at, (_, _, renderer))| {
            let renderer = (*renderer)?;
            Some(PreviewPart { mesh: renderer.mesh, material: renderer.material, transform: world(at) })
        })
        .collect()
}

/// One preview per asset and size, made the first time it is asked for, as
/// the editor shows them in the Project panel, reference fields and the
/// object picker. Each looks as [`PreviewOptions::default`] says, at the
/// size asked.
#[derive(Default)]
pub struct AssetPreviews {
    /// `None` for an asset with nothing to show, so it is not tried again.
    made: HashMap<(String, u32), Option<PreviewId>>,
}

impl AssetPreviews {
    /// The preview of an asset at `size` pixels, or `None` while it is drawn
    /// or when the asset has nothing to show.
    pub fn image(
        &mut self,
        content: &mut Content,
        renderer: &mut Renderer,
        reference: &str,
        size: u32,
    ) -> Option<PreviewImage> {
        let key = (reference.to_string(), size);
        let id = match self.made.get(&key) {
            Some(made) => *made,
            None => {
                let made = preview_parts(content, renderer, reference)
                    .ok()
                    .filter(|parts| !parts.is_empty())
                    .map(|parts| renderer.add_preview(parts, PreviewOptions { size, ..PreviewOptions::default() }));
                self.made.insert(key, made);
                made
            }
        };
        renderer.preview(id?)
    }

    /// Whether a preview of an asset at `size` was asked for already, so
    /// asking again loads nothing: for spreading new ones over frames.
    pub fn has(&self, reference: &str, size: u32) -> bool {
        self.made.contains_key(&(reference.to_string(), size))
    }

    /// Whether an asset can have a preview, by its kind.
    pub fn shows(kind: AssetKind) -> bool {
        matches!(kind, AssetKind::Material | AssetKind::Prefab | AssetKind::Model)
    }

    /// Draws an asset's previews again, as after it changed, or lets them go
    /// when it has nothing to show any more.
    pub fn refresh(&mut self, content: &mut Content, renderer: &mut Renderer, reference: &str) {
        let sizes: Vec<(u32, Option<PreviewId>)> =
            self.made.iter().filter(|((r, _), _)| r == reference).map(|((_, size), id)| (*size, *id)).collect();
        for (size, id) in sizes {
            let parts = preview_parts(content, renderer, reference).ok().filter(|parts| !parts.is_empty());
            match (id, parts) {
                (Some(id), Some(parts)) => {
                    renderer.set_preview(id, Some(parts), PreviewOptions { size, ..PreviewOptions::default() });
                }
                (id, _) => {
                    if let Some(id) = id {
                        renderer.free_preview(id);
                    }
                    self.made.remove(&(reference.to_string(), size));
                }
            }
        }
    }

    /// Lets every preview go, as when the project's assets change.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for id in self.made.drain().filter_map(|(_, id)| id) {
            renderer.free_preview(id);
        }
    }
}

/// The previews a game's scripts asked for (see `Voxear.Preview`), and
/// those waiting to be written to PNG files once their pixels are back.
#[derive(Default)]
pub struct ScriptPreviews {
    made: HashSet<PreviewId>,
    saving: Vec<(PreviewId, PathBuf)>,
}

impl ScriptPreviews {
    /// Asks for a preview of an asset, by GUID or project path, drawn as
    /// `options` say.
    pub fn create(
        &mut self,
        content: &mut Content,
        renderer: &mut Renderer,
        reference: &str,
        options: PreviewOptions,
    ) -> Result<PreviewId> {
        let parts = preview_parts(content, renderer, reference)?;
        if parts.is_empty() {
            bail!("`{reference}` draws nothing");
        }
        let id = renderer.add_preview(parts, options);
        self.made.insert(id);
        Ok(id)
    }

    /// Where one of these previews is, once drawn; `None` for one that is
    /// not theirs.
    pub fn image(&mut self, renderer: &mut Renderer, id: PreviewId) -> Option<Option<PreviewImage>> {
        self.made.contains(&id).then(|| renderer.preview(id))
    }

    /// Writes a preview to `path` as a PNG once it is drawn and read back.
    pub fn save(&mut self, renderer: &mut Renderer, id: PreviewId, path: &Path) -> Result<()> {
        if !self.made.contains(&id) {
            bail!("there is no such preview");
        }
        renderer.read_preview(id);
        self.saving.push((id, path.to_path_buf()));
        Ok(())
    }

    pub fn free(&mut self, renderer: &mut Renderer, id: PreviewId) {
        if self.made.remove(&id) {
            renderer.free_preview(id);
            self.saving.retain(|(saving, _)| *saving != id);
        }
    }

    /// Writes the previews whose pixels are back, and logs any that fail.
    pub fn finish_saves(&mut self, renderer: &mut Renderer) {
        self.saving.retain(|(id, path)| {
            let Some(pixels) = renderer.preview_pixels(*id) else { return true };
            match pixels.save_png(path) {
                Ok(()) => tracing::info!("saved a preview to {}", path.display()),
                Err(error) => tracing::error!("could not save a preview to {}: {error:#}", path.display()),
            }
            false
        });
    }

    /// Lets every preview go, as when the game stops.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        for id in self.made.drain() {
            renderer.free_preview(id);
        }
        self.saving.clear();
    }
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Vec3;
    use voxear_render::{MaterialHandle, MeshHandle};

    use super::*;

    #[test]
    fn a_prefabs_parts_sit_where_their_parents_put_them() {
        let renderer = Some(MeshRenderer { mesh: MeshHandle(1), material: MaterialHandle(2) });
        let objs = [
            (None, Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)), None),
            (Some(0), Mat4::from_translation(Vec3::new(0.0, 2.0, 0.0)), renderer),
            (Some(1), Mat4::from_scale(Vec3::splat(2.0)), renderer),
        ];
        let parts = prefab_parts(&objs);
        assert_eq!(parts.len(), 2, "only Objs with a renderer");
        assert_eq!(parts[0].transform.transform_point3(Vec3::ZERO), Vec3::new(1.0, 2.0, 0.0));
        assert_eq!(parts[1].transform.transform_point3(Vec3::X), Vec3::new(3.0, 2.0, 0.0));
    }
}
